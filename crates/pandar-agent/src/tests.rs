use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use super::*;
use crate::machine::{
    BambuMachineGateway, discovery::DiscoveredPrinter, file_transfer::FakeMachineFileTransfer,
    mqtt::FakeMqttTransport, runtime::test_support::TestRuntimeBambuMachineGateway,
};
use crate::protocol::agent::v1::{
    AgentCameraEvent, AgentCapability, AgentEvent, HubCameraCommand, HubCommand, LinkPrinter,
    agent_control_server::{AgentControl, AgentControlServer},
    hub_command,
};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

mod startup;
mod tls;

#[test]
fn parses_agent_cli_config() {
    let agent_id = uuid::Uuid::new_v4().to_string();
    let tenant_id = uuid::Uuid::new_v4().to_string();
    let config = AgentConfig::parse_from([
        "pandar-agent",
        "--hub-grpc-url",
        "http://hub.internal:50051",
        "--agent-name",
        "garage",
        "--agent-id",
        &agent_id,
        "--tenant-id",
        &tenant_id,
        "--agent-credential",
        "pandar_ac_test",
        "--agent-version",
        "9.8.7",
        "--printers",
        r#"[{"host":"192.0.2.10","serial":"SERIAL","access_code":"12345678"}]"#,
    ]);

    assert_eq!(config.hub_grpc_url, "http://hub.internal:50051");
    assert_eq!(config.hub_api_url, None);
    assert_eq!(config.agent_name, "garage");
    assert_eq!(config.agent_id, agent_id);
    assert_eq!(config.tenant_id, tenant_id);
    assert_eq!(config.agent_credential, "pandar_ac_test");
    assert_eq!(config.agent_version, "9.8.7");
    assert_eq!(
        config.printers,
        r#"[{"host":"192.0.2.10","serial":"SERIAL","access_code":"12345678"}]"#
    );
    assert_eq!(config.artifact_root, std::path::PathBuf::from("."));
}

#[tokio::test]
async fn invalid_printer_config_fails_before_reconnect_loop() {
    let config = AgentConfig {
        printers: r#"[{"host":"192.0.2.10","serial":"","access_code":"12345678"}]"#.to_owned(),
        ..test_config()
    };

    let err = startup_printers(&config).await.unwrap_err();

    assert!(format!("{err:#}").contains("PANDAR_PRINTERS"));
    assert!(format!("{err:#}").contains("serial"));
}

#[test]
fn startup_summary_names_hub_and_agent() {
    let config = AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: env!("CARGO_PKG_VERSION").to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    };

    assert_eq!(
        startup_summary(&config),
        "agent garage will connect to http://hub.internal:50051"
    );
}

#[test]
fn hello_event_has_agent_identity_version_and_exact_capability() {
    let config = test_config();

    let event = hello_event(&config);

    assert_eq!(event.agent_id, config.agent_id.to_string());
    assert_eq!(event.tenant_id, config.tenant_id.to_string());
    assert_eq!(event.event_id, "hello");
    assert_eq!(
        event.event,
        Some(agent_event::Event::Hello(AgentHello {
            name: "garage".to_owned(),
            version: "9.8.7".to_owned(),
            credential: "pandar_ac_test".to_owned(),
            capabilities: vec![AgentCapability::HandlePrintError as i32],
        }))
    );
}

#[tokio::test]
async fn ended_command_stream_preserves_runtime_linked_printer_for_reconnect() {
    let gateway = TestRuntimeBambuMachineGateway::new(
        Vec::new(),
        FakeMachineFileTransfer::default(),
        Duration::from_secs(1),
    );
    gateway
        .push_command_transport(FakeMqttTransport::with_reports([
            get_version_report("X1 Carbon"),
            runtime_state_report("READY"),
            get_version_report("X1 Carbon"),
            runtime_state_report("IDLE"),
        ]))
        .await;
    gateway
        .set_discovered_printers(vec![DiscoveredPrinter {
            serial_number: Some("SERIAL123".to_owned()),
            host: "192.0.2.10".to_owned(),
            name: Some("office".to_owned()),
            model: Some("X1 Carbon".to_owned()),
            source: "ssdp",
        }])
        .await;
    let config = test_config();
    let (sender, mut events) = mpsc::channel(8);

    handle_command_stream_with_gateway(
        &config,
        &gateway,
        &sender,
        tokio_stream::iter([Ok(link_printer_command())]),
    )
    .await
    .unwrap();
    assert!(received_success_result(&mut events));

    let (reconnected_sender, _) = mpsc::channel(8);
    handle_command_stream_with_gateway(
        &config,
        &gateway,
        &reconnected_sender,
        tokio_stream::iter([]),
    )
    .await
    .unwrap();

    let snapshots = gateway.refresh_printers().await.unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].snapshot.serial, "SERIAL123");
    assert_eq!(snapshots[0].snapshot.state, "IDLE");
}

#[tokio::test]
async fn run_once_does_not_open_camera_stream_before_camera_command() {
    let reverse_camera_called = Arc::new(AtomicBool::new(false));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let service = TestAgentControlService {
        reverse_camera_called: reverse_camera_called.clone(),
    };
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(AgentControlServer::new(service))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    let config = AgentConfig {
        hub_grpc_url: format!("http://127.0.0.1:{}", address.port()),
        ..test_config()
    };
    let gateway = crate::machine::runtime::RuntimeBambuMachineGateway::new(
        config.clone(),
        Vec::new(),
        Duration::from_secs(1),
    );

    let outcome = run_once(config, &gateway).await.unwrap();

    assert_eq!(outcome, RunOutcome::ConnectedThenEnded);
    assert!(!reverse_camera_called.load(Ordering::SeqCst));
}

#[test]
fn backoff_doubles_and_caps() {
    let mut backoff = ReconnectBackoff::new();

    let delays: Vec<_> = (0..8).map(|_| backoff.next_delay()).collect();

    assert_eq!(
        delays,
        [
            Duration::from_secs(1),
            Duration::from_secs(2),
            Duration::from_secs(4),
            Duration::from_secs(8),
            Duration::from_secs(16),
            Duration::from_secs(30),
            Duration::from_secs(30),
            Duration::from_secs(30),
        ]
    );
}

#[test]
fn backoff_reset_returns_to_one_second() {
    let mut backoff = ReconnectBackoff::new();

    assert_eq!(backoff.next_delay(), Duration::from_secs(1));
    assert_eq!(backoff.next_delay(), Duration::from_secs(2));
    backoff.reset();

    assert_eq!(backoff.next_delay(), Duration::from_secs(1));
}

#[test]
fn heartbeat_interval_is_fifteen_seconds() {
    assert_eq!(HEARTBEAT_INTERVAL, Duration::from_secs(15));
}

fn test_config() -> AgentConfig {
    AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    }
}

fn get_version_report(model: &str) -> serde_json::Value {
    serde_json::to_value(TestGetVersionReport {
        info: TestGetVersionInfo {
            command: "get_version",
            module: [TestGetVersionModule {
                name: "ota",
                product_name: model,
            }],
        },
    })
    .unwrap()
}

fn runtime_state_report(state: &str) -> serde_json::Value {
    serde_json::to_value(TestRuntimeStateReport {
        print: TestRuntimePrintReport {
            state,
            ams: TestRuntimeAmsReport {
                ams: [TestRuntimeAmsUnit {
                    id: "0",
                    tray: [TestRuntimeAmsTray {
                        id: "0",
                        tray_type: "PLA",
                    }],
                }],
            },
        },
    })
    .unwrap()
}

#[derive(Debug, Serialize)]
struct TestGetVersionReport<'a> {
    info: TestGetVersionInfo<'a>,
}

#[derive(Debug, Serialize)]
struct TestGetVersionInfo<'a> {
    command: &'static str,
    module: [TestGetVersionModule<'a>; 1],
}

#[derive(Debug, Serialize)]
struct TestGetVersionModule<'a> {
    name: &'static str,
    product_name: &'a str,
}

#[derive(Debug, Serialize)]
struct TestRuntimeStateReport<'a> {
    print: TestRuntimePrintReport<'a>,
}

#[derive(Debug, Serialize)]
struct TestRuntimePrintReport<'a> {
    state: &'a str,
    ams: TestRuntimeAmsReport,
}

#[derive(Debug, Serialize)]
struct TestRuntimeAmsReport {
    ams: [TestRuntimeAmsUnit; 1],
}

#[derive(Debug, Serialize)]
struct TestRuntimeAmsUnit {
    id: &'static str,
    tray: [TestRuntimeAmsTray; 1],
}

#[derive(Debug, Serialize)]
struct TestRuntimeAmsTray {
    id: &'static str,
    tray_type: &'static str,
}

fn link_printer_command() -> HubCommand {
    HubCommand {
        command_id: uuid::Uuid::new_v4().to_string(),
        command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
            host: "192.0.2.10".to_owned(),
            access_code: "12345678".to_owned(),
            name: "office".to_owned(),
            printer_type: "BambuLab".to_owned(),
        })),
    }
}

fn received_success_result(events: &mut mpsc::Receiver<AgentEvent>) -> bool {
    let mut received_success = false;
    while let Ok(event) = events.try_recv() {
        if matches!(
            event.event,
            Some(agent_event::Event::CommandResult(result)) if result.success
        ) {
            received_success = true;
        }
    }
    received_success
}

struct TestAgentControlService {
    reverse_camera_called: Arc<AtomicBool>,
}

#[tonic::async_trait]
impl AgentControl for TestAgentControlService {
    type ReverseConnectStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<HubCommand, Status>> + Send>>;
    type ReverseCameraStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<HubCameraCommand, Status>> + Send>>;

    async fn reverse_connect(
        &self,
        request: Request<tonic::Streaming<AgentEvent>>,
    ) -> Result<Response<Self::ReverseConnectStream>, Status> {
        let mut inbound = request.into_inner();
        let _ = inbound.message().await?;
        Ok(Response::new(Box::pin(tokio_stream::empty())))
    }

    async fn reverse_camera(
        &self,
        _request: Request<tonic::Streaming<AgentCameraEvent>>,
    ) -> Result<Response<Self::ReverseCameraStream>, Status> {
        self.reverse_camera_called.store(true, Ordering::SeqCst);
        Ok(Response::new(Box::pin(tokio_stream::empty())))
    }
}
