use serde::Deserialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
};

use super::*;
use pandar_protocol::agent::v1::ReloadPrinterConnection;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct ReloadResult {
    #[serde(rename = "type")]
    kind: String,
    printer_id: String,
    serial_number: String,
    host: String,
}

#[tokio::test]
async fn reload_printer_connection_fetches_latest_saved_endpoint() {
    let server = TestHubPrinterServer::start(saved_printer_body()).await;
    let config = AgentConfig {
        hub_api_url: Some(server.base_url()),
        ..test_config()
    };
    let gateway = LinkGateway::success(snapshot(
        "SERIAL123",
        "Office X1C",
        Some("X1 Carbon"),
        "READY",
    ));
    let command_id = uuid::Uuid::new_v4().to_string();
    let (sender, mut receiver) = mpsc::channel(3);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        reload_command(command_id.clone()),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    let snapshot = receiver.recv().await.unwrap();
    let Some(agent_event::Event::PrinterSnapshot(snapshot)) = snapshot.event else {
        panic!("expected authoritative printer snapshot");
    };
    assert!(snapshot.connection_authoritative);
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert!(result.success);
            assert!(!result.result_json.contains("SECRET-LINK-CODE"));
            assert_eq!(
                serde_json::from_str::<ReloadResult>(&result.result_json).unwrap(),
                ReloadResult {
                    kind: "printer_connection_reload".to_owned(),
                    printer_id: "printer-1".to_owned(),
                    serial_number: "SERIAL123".to_owned(),
                    host: "192.0.2.10".to_owned(),
                }
            );
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(gateway.linked_endpoints().await.len(), 1);
    assert!(receiver.recv().await.is_none());
    server.assert_request().await;
}

#[tokio::test]
async fn reload_printer_connection_redacts_saved_access_code_on_failure() {
    let server = TestHubPrinterServer::start(saved_printer_body()).await;
    let config = AgentConfig {
        hub_api_url: Some(server.base_url()),
        ..test_config()
    };
    let gateway = LinkGateway::failure("SECRET-LINK-CODE");
    let command_id = uuid::Uuid::new_v4().to_string();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        reload_command(command_id.clone()),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert!(!result.success);
            assert!(result.error.contains("replace runtime printer connection"));
            assert!(result.error.contains("[REDACTED_ACCESS_CODE]"));
            assert!(!result.error.contains("SECRET-LINK-CODE"));
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
    server.assert_request().await;
}

fn reload_command(command_id: String) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::ReloadPrinterConnection(
            ReloadPrinterConnection {
                printer_id: "printer-1".to_owned(),
                serial_number: "SERIAL123".to_owned(),
            },
        )),
    }
}

fn saved_printer_body() -> String {
    serde_json::json!({
        "printers": [{
            "serial": "SERIAL123",
            "host": "192.0.2.10",
            "access_code": "SECRET-LINK-CODE",
            "name": "Office X1C",
            "model": "X1 Carbon"
        }]
    })
    .to_string()
}

struct TestHubPrinterServer {
    address: std::net::SocketAddr,
    request_receiver: oneshot::Receiver<()>,
}

impl TestHubPrinterServer {
    async fn start(body: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (request_sender, request_receiver) = oneshot::channel();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0; 4096];
            let read = stream.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..read]);
            assert!(request.starts_with("GET /api/v1/agents/agent-id/printers HTTP/1.1"));
            assert!(request.contains("Bearer pandar_ac_test"));
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            request_sender.send(()).unwrap();
        });
        Self {
            address,
            request_receiver,
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    async fn assert_request(self) {
        self.request_receiver.await.unwrap();
    }
}
