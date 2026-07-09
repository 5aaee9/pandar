use std::{
    pin::Pin,
    sync::{Mutex, Once},
};

use rcgen::{CertifiedKey, generate_simple_self_signed};
use tokio::{net::TcpListener, sync::oneshot};
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tonic::transport::{Certificate, ClientTlsConfig, Identity, Server, ServerTlsConfig};
use tonic::{Request, Response, Status};

use crate::{
    hello_event,
    protocol::agent::v1::{
        AgentCameraEvent, AgentEvent, HubCameraCommand, HubCommand,
        agent_control_client::AgentControlClient,
        agent_control_server::{AgentControl, AgentControlServer},
    },
};

#[tokio::test]
async fn agent_control_client_connects_to_tls_grpc_hub() {
    install_rustls_test_provider();
    let CertifiedKey { cert, signing_key } =
        generate_simple_self_signed(["localhost".to_owned()]).unwrap();
    let cert_pem = cert.pem();
    let key_pem = signing_key.serialize_pem();
    let (event_sender, event_receiver) = oneshot::channel();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let service = TestTlsAgentControl {
        event_sender: Mutex::new(Some(event_sender)),
    };
    tokio::spawn(async move {
        Server::builder()
            .tls_config(ServerTlsConfig::new().identity(Identity::from_pem(&cert_pem, &key_pem)))
            .unwrap()
            .add_service(AgentControlServer::new(service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    let endpoint =
        tonic::transport::Endpoint::from_shared(format!("https://localhost:{}", address.port()))
            .unwrap()
            .tls_config(ClientTlsConfig::new().ca_certificate(Certificate::from_pem(cert.pem())))
            .unwrap();
    let mut client = AgentControlClient::new(endpoint.connect().await.unwrap());
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let event = hello_event(&super::test_config());
    sender.send(event.clone()).await.unwrap();
    drop(sender);

    let _response = client
        .reverse_connect(Request::new(ReceiverStream::new(receiver)))
        .await
        .unwrap();

    assert_eq!(event_receiver.await.unwrap(), event);
}

fn install_rustls_test_provider() {
    static INSTALL_PROVIDER: Once = Once::new();
    INSTALL_PROVIDER.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

struct TestTlsAgentControl {
    event_sender: Mutex<Option<oneshot::Sender<AgentEvent>>>,
}

#[tonic::async_trait]
impl AgentControl for TestTlsAgentControl {
    type ReverseConnectStream = Pin<Box<dyn Stream<Item = Result<HubCommand, Status>> + Send>>;
    type ReverseCameraStream = Pin<Box<dyn Stream<Item = Result<HubCameraCommand, Status>> + Send>>;

    async fn reverse_connect(
        &self,
        request: Request<tonic::Streaming<AgentEvent>>,
    ) -> Result<Response<Self::ReverseConnectStream>, Status> {
        let event = request.into_inner().message().await?.unwrap();
        self.event_sender
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(event)
            .unwrap();
        Ok(Response::new(Box::pin(tokio_stream::empty())))
    }

    async fn reverse_camera(
        &self,
        _request: Request<tonic::Streaming<AgentCameraEvent>>,
    ) -> Result<Response<Self::ReverseCameraStream>, Status> {
        Ok(Response::new(Box::pin(tokio_stream::empty())))
    }
}
