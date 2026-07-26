use std::{
    future, io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use futures_util::{Stream, StreamExt, stream};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore},
    time::Sleep,
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};
use tonic::transport::server::Connected;

mod peer;

use peer::{PeerConnections, PeerPermit};

const GRPC_CONNECTION_SETUP_TIMEOUT: Duration = Duration::from_secs(10);
const HTTP2_CLIENT_PREFACE_BYTES: usize = 24;

struct LimitedTcpStream {
    inner: TcpStream,
    _permit: OwnedSemaphorePermit,
    _peer_permit: PeerPermit,
}

struct PrefaceDeadline<I> {
    inner: I,
    deadline: Pin<Box<Sleep>>,
    remaining_preface_bytes: usize,
}

impl<I> PrefaceDeadline<I> {
    fn new(inner: I, timeout: Duration) -> Self {
        Self {
            inner,
            deadline: Box::pin(tokio::time::sleep(timeout)),
            remaining_preface_bytes: HTTP2_CLIENT_PREFACE_BYTES,
        }
    }
}

enum ConnectionIo {
    Plain(PrefaceDeadline<LimitedTcpStream>),
    Tls(Box<PrefaceDeadline<TlsStream<LimitedTcpStream>>>),
}

pub(crate) struct GrpcConnection {
    io: ConnectionIo,
    peer_permit: PeerPermit,
}

#[derive(Clone)]
pub(crate) struct GrpcConnectInfo {
    peer_permit: PeerPermit,
}

impl GrpcConnectInfo {
    pub(crate) fn mark_authenticated(
        &self,
        tenant_id: pandar_core::TenantId,
        agent_id: pandar_core::AgentId,
    ) -> bool {
        self.peer_permit.mark_authenticated(tenant_id, agent_id)
    }
}

impl AsyncRead for GrpcConnection {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match &mut self.io {
            ConnectionIo::Plain(stream) => Pin::new(stream).poll_read(context, buffer),
            ConnectionIo::Tls(stream) => Pin::new(stream).poll_read(context, buffer),
        }
    }
}

impl<I> AsyncRead for PrefaceDeadline<I>
where
    I: AsyncRead + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.remaining_preface_bytes > 0 && self.deadline.as_mut().poll(context).is_ready() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "gRPC HTTP/2 client preface timed out",
            )));
        }
        let filled_before = buffer.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(context, buffer);
        if matches!(result, Poll::Ready(Ok(()))) {
            let received = buffer.filled().len().saturating_sub(filled_before);
            self.remaining_preface_bytes = self.remaining_preface_bytes.saturating_sub(received);
        }
        result
    }
}

impl AsyncRead for LimitedTcpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(context, buffer)
    }
}

macro_rules! delegate_async_write {
    ($self:ident, $context:ident, $method:ident $(, $argument:ident)?) => {
        match &mut $self.io {
            ConnectionIo::Plain(stream) => Pin::new(stream).$method($context $(, $argument)?),
            ConnectionIo::Tls(stream) => Pin::new(stream).$method($context $(, $argument)?),
        }
    };
}

impl AsyncWrite for GrpcConnection {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        delegate_async_write!(self, context, poll_write, buffer)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        delegate_async_write!(self, context, poll_flush)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        delegate_async_write!(self, context, poll_shutdown)
    }

    fn is_write_vectored(&self) -> bool {
        match &self.io {
            ConnectionIo::Plain(stream) => stream.is_write_vectored(),
            ConnectionIo::Tls(stream) => stream.is_write_vectored(),
        }
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffers: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        delegate_async_write!(self, context, poll_write_vectored, buffers)
    }
}

impl<I> AsyncWrite for PrefaceDeadline<I>
where
    I: AsyncWrite + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(context, buffer)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(context)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffers: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write_vectored(context, buffers)
    }
}

impl AsyncWrite for LimitedTcpStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(context, buffer)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(context)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffers: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write_vectored(context, buffers)
    }
}

impl Connected for GrpcConnection {
    type ConnectInfo = GrpcConnectInfo;

    fn connect_info(&self) -> Self::ConnectInfo {
        GrpcConnectInfo {
            peer_permit: self.peer_permit.clone(),
        }
    }
}

pub(crate) fn incoming(
    listener: TcpListener,
    max_connections: usize,
    max_unauthenticated_connections_per_peer: usize,
    tls_config: Option<Arc<rustls::ServerConfig>>,
) -> impl Stream<Item = io::Result<GrpcConnection>> {
    incoming_with_timeout(
        listener,
        max_connections,
        tls_config,
        GRPC_CONNECTION_SETUP_TIMEOUT,
        max_unauthenticated_connections_per_peer,
    )
}

fn incoming_with_timeout(
    listener: TcpListener,
    max_connections: usize,
    tls_config: Option<Arc<rustls::ServerConfig>>,
    setup_timeout: Duration,
    max_connections_per_peer: usize,
) -> impl Stream<Item = io::Result<GrpcConnection>> {
    let permits = Arc::new(Semaphore::new(max_connections));
    let peer_connections = PeerConnections::new(max_connections_per_peer);
    stream::unfold(
        (listener, permits, peer_connections, tls_config),
        move |(listener, permits, peer_connections, tls_config)| async move {
            let permit = permits
                .clone()
                .acquire_owned()
                .await
                .expect("gRPC connection semaphore remains open");
            let accepted = listener.accept().await;
            let attempt_tls_config = tls_config.clone();
            let attempt_peer_connections = Arc::clone(&peer_connections);
            let attempt = async move {
                let (inner, peer_addr) = match accepted {
                    Ok(accepted) => accepted,
                    Err(error) => return Some(Err(error)),
                };
                let Some(peer_permit) = attempt_peer_connections.try_acquire(peer_addr.ip()) else {
                    tracing::debug!(%peer_addr, "rejected gRPC per-peer connection limit");
                    return None;
                };
                let stream = LimitedTcpStream {
                    inner,
                    _permit: permit,
                    _peer_permit: peer_permit.clone(),
                };
                let io = match attempt_tls_config {
                    Some(config) => {
                        let handshake = TlsAcceptor::from(config).accept(stream);
                        match tokio::time::timeout(setup_timeout, handshake).await {
                            Ok(Ok(stream)) => {
                                ConnectionIo::Tls(Box::new(PrefaceDeadline::new(
                                    stream,
                                    setup_timeout,
                                )))
                            }
                            Ok(Err(error)) => {
                                tracing::debug!(error = %error, %peer_addr, "rejected gRPC TLS connection");
                                return None;
                            }
                            Err(error) => {
                                tracing::debug!(error = %error, %peer_addr, "gRPC TLS handshake timed out");
                                return None;
                            }
                        }
                    }
                    None => ConnectionIo::Plain(PrefaceDeadline::new(stream, setup_timeout)),
                };
                Some(Ok(GrpcConnection { io, peer_permit }))
            };
            Some((
                attempt,
                (listener, permits, peer_connections, tls_config),
            ))
        },
    )
    .buffer_unordered(max_connections)
    .filter_map(future::ready)
}

#[cfg(test)]
mod tests;
