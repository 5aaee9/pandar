use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
};

use crate::{AgentConfig, machine::BambuPrinterEndpoint, startup::startup_printers};

#[tokio::test]
async fn startup_printers_loads_saved_hub_connections() {
    let server = TestHubPrinterServer::start(
        "agent-id",
        "pandar_ac_test",
        r#"{"printers":[{"serial":"SERIAL123","host":"192.0.2.10","access_code":"12345678","name":"Office X1C","model":"X1 Carbon"}]}"#,
    )
    .await;
    let config = AgentConfig {
        hub_api_url: Some(server.base_url()),
        ..super::test_config()
    };

    let printers = startup_printers(&config).await.unwrap();

    assert_eq!(
        printers,
        vec![BambuPrinterEndpoint {
            host: "192.0.2.10".to_owned(),
            serial: "SERIAL123".to_owned(),
            access_code: "12345678".to_owned(),
            model: Some("X1 Carbon".to_owned()),
            name: Some("Office X1C".to_owned()),
        }]
    );
    let request = server.request().await;
    assert_eq!(request.path, "/api/v1/agents/agent-id/printers");
    assert_eq!(
        request.authorization.as_deref(),
        Some("Bearer pandar_ac_test")
    );
}

struct TestHubPrinterServer {
    address: std::net::SocketAddr,
    request_receiver: oneshot::Receiver<TestHubPrinterRequest>,
}

impl TestHubPrinterServer {
    async fn start(agent_id: &str, credential: &str, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let expected_path = format!("/api/v1/agents/{agent_id}/printers");
        let expected_authorization = format!("Bearer {credential}");
        let (request_sender, request_receiver) = oneshot::channel();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0; 4096];
            let read = stream.read(&mut buffer).await.unwrap();
            let raw = String::from_utf8_lossy(&buffer[..read]);
            let mut lines = raw.lines();
            let request_line = lines.next().unwrap();
            let path = request_line.split_whitespace().nth(1).unwrap().to_owned();
            assert_eq!(path, expected_path);
            let authorization = lines.find_map(|line| {
                line.strip_prefix("authorization: ")
                    .or_else(|| line.strip_prefix("Authorization: "))
                    .map(ToOwned::to_owned)
            });
            assert_eq!(
                authorization.as_deref(),
                Some(expected_authorization.as_str())
            );
            let _ = request_sender.send(TestHubPrinterRequest {
                path,
                authorization,
            });
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });

        Self {
            address,
            request_receiver,
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    async fn request(self) -> TestHubPrinterRequest {
        self.request_receiver.await.unwrap()
    }
}

struct TestHubPrinterRequest {
    path: String,
    authorization: Option<String>,
}
