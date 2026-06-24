use std::sync::Arc;

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Mutex,
};

use super::test_config;
use crate::commands::{ArtifactReader, HubArtifactReader, artifact_download_url};

#[test]
fn artifact_download_url_uses_explicit_or_ordinary_http_hub_url() {
    assert_eq!(
        artifact_download_url(
            Some("https://api.example.test/base/"),
            "http://grpc.example.test:50051",
            "/api/v1/agents/agent/artifacts/artifact"
        )
        .unwrap(),
        "https://api.example.test/base/api/v1/agents/agent/artifacts/artifact"
    );
    assert_eq!(
        artifact_download_url(
            None,
            "http://hub.internal:8080",
            "/api/v1/agents/agent/artifacts/artifact"
        )
        .unwrap(),
        "http://hub.internal:8080/api/v1/agents/agent/artifacts/artifact"
    );

    let err = artifact_download_url(None, "grpc://hub.internal:50051", "/artifact")
        .unwrap_err()
        .to_string();
    assert!(err.contains("PANDAR_HUB_API_URL"));
}

#[tokio::test]
async fn hub_artifact_reader_fetches_with_bearer_agent_credential() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let observed_for_server = observed.clone();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 2048];
        let read = socket.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..read]);
        let auth = request
            .lines()
            .find_map(|line| line.strip_prefix("authorization: "))
            .or_else(|| {
                request
                    .lines()
                    .find_map(|line| line.strip_prefix("Authorization: "))
            })
            .unwrap_or_default()
            .to_string();
        observed_for_server.lock().await.push(auth);
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\ncontent-length: 9\r\ncontent-type: model/3mf\r\n\r\nhub-bytes",
            )
            .await
            .unwrap();
    });
    let config = crate::AgentConfig {
        hub_api_url: Some(base_url),
        ..test_config()
    };
    let reader = HubArtifactReader::new(&config);

    let bytes = reader
        .read_artifact("/api/v1/agents/agent-id/artifacts/artifact-1")
        .await
        .unwrap();

    assert_eq!(bytes, b"hub-bytes");
    assert_eq!(
        observed.lock().await.as_slice(),
        &["Bearer pandar_ac_test".to_string()]
    );
    server.abort();
}
