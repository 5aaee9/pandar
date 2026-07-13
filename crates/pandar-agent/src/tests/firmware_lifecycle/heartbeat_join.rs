use super::*;

#[tokio::test]
async fn firmware_run_once_preserves_heartbeat_panic_join_error() {
    let connected = Arc::new(Notify::new());
    let inbound_closed = Arc::new(Notify::new());
    let end_commands = Arc::new(Notify::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn({
        let connected = Arc::clone(&connected);
        let inbound_closed = Arc::clone(&inbound_closed);
        let end_commands = Arc::clone(&end_commands);
        async move {
            tonic::transport::Server::builder()
                .add_service(AgentControlServer::new(EofAgentControlService {
                    connected,
                    inbound_closed,
                    end_commands,
                }))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        }
    });
    let config = AgentConfig {
        hub_grpc_url: format!("http://127.0.0.1:{}", address.port()),
        ..test_config()
    };
    let gateway = Arc::new(crate::machine::runtime::RuntimeBambuMachineGateway::new(
        config.clone(),
        Vec::new(),
        Duration::from_secs(1),
    ));
    let heartbeat = gateway.panic_heartbeat_for_test().await;
    let task = tokio::spawn(run_once(config, Arc::clone(&gateway)));
    connected.notified().await;
    heartbeat.wait_until_started().await;

    heartbeat.panic();
    heartbeat.wait_until_unwound().await;
    end_commands.notify_one();
    let error = tokio::time::timeout(Duration::from_millis(250), task)
        .await
        .expect("heartbeat panic teardown must finish")
        .unwrap()
        .unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains("join Agent heartbeat task"));
    assert!(message.contains("firmware heartbeat panic sentinel"));
    server.abort();
    let _ = server.await;
}
