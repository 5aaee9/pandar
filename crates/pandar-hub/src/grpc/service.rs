use tonic::{Request, Response, Status};

use crate::{
    grpc_connection_limit::GrpcConnectInfo,
    protocol::agent::v1::{AgentCameraEvent, AgentEvent, agent_control_server::AgentControl},
};

use super::{AgentControlService, CameraResponseStream, ResponseStream};

#[tonic::async_trait]
impl AgentControl for AgentControlService {
    type ReverseConnectStream = ResponseStream;
    type ReverseCameraStream = CameraResponseStream;

    async fn reverse_connect(
        &self,
        request: Request<tonic::Streaming<AgentEvent>>,
    ) -> Result<Response<Self::ReverseConnectStream>, Status> {
        let connect_info = request.extensions().get::<GrpcConnectInfo>().cloned();
        self.connect_stream(request.into_inner(), connect_info)
            .await
            .map(Response::new)
    }

    async fn reverse_camera(
        &self,
        request: Request<tonic::Streaming<AgentCameraEvent>>,
    ) -> Result<Response<Self::ReverseCameraStream>, Status> {
        let connect_info = request.extensions().get::<GrpcConnectInfo>().cloned();
        self.connect_camera_stream(request.into_inner(), connect_info)
            .await
            .map(Response::new)
    }
}
