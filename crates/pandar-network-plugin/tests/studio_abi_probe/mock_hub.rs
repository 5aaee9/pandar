#[path = "mock_hub/account_race.rs"]
mod account_race;
#[path = "mock_hub/admission.rs"]
mod admission;
#[path = "mock_hub/connection.rs"]
mod connection;
#[path = "mock_hub/firmware_compat.rs"]
mod firmware_compat;
#[path = "mock_hub/freshness.rs"]
mod freshness;
#[path = "mock_hub/native.rs"]
mod native;
#[path = "mock_hub/operations.rs"]
mod operations;
#[path = "mock_hub/presence.rs"]
mod presence;
#[path = "mock_hub/responses.rs"]
mod responses;
#[path = "mock_hub/server.rs"]
mod server;
#[path = "mock_hub/synchronization.rs"]
mod synchronization;
#[path = "mock_hub/transport.rs"]
mod transport;

pub(super) use operations::required_device_feature_presence_matches;
use std::{
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Instant,
};
use synchronization::ProbeStart;
use transport::{read_request_until, write_response};

pub(super) fn next_request(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    method: &str,
    path: &str,
) -> (TcpStream, String) {
    let waiting_for = format!("{method} {path}");
    loop {
        let (mut stream, request) = read_request_until(listener, stop, deadline, &waiting_for)
            .unwrap_or_else(|| panic!("Studio ABI probe exited before {waiting_for}"));
        if firmware_compat::try_respond(&mut stream, &request) {
            continue;
        }
        return (stream, request);
    }
}

pub(super) fn next_request_allow_ready(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    method: &str,
    path: &str,
) -> (TcpStream, String) {
    loop {
        let (mut stream, request) = next_request(listener, stop, deadline, method, path);
        if request.lines().next() == Some("GET /readyz HTTP/1.1") {
            write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                r#"{"status":"ready","checks":{}}"#,
            );
            continue;
        }
        return (stream, request);
    }
}

#[derive(Clone, Copy)]
pub(super) enum MockMode {
    Success,
    ConnectionReadiness,
    BackgroundTimeout,
    AuthRejected,
    PrinterPresence,
    AccountTransition,
    AccountExchangeRace,
    TokenRotation,
    TokenRotationOffline,
    FreshnessClaim,
    FirmwareClaimRace,
    CallbackOrder,
    RequestAdmission,
    CameraUnavailable,
    NoAuthRecovery,
    OfficialNoAuthRecovery,
    OfficialNoAuthLogoutRecovery,
    Failure,
    NativePrintError,
    AxisFeatures,
}

pub(super) struct MockHub {
    pub(super) url: String,
    handle: thread::JoinHandle<()>,
    stop: Arc<AtomicBool>,
    start: ProbeStart,
}

impl MockHub {
    pub(super) fn start(&self, deadline: Instant) {
        self.start.arm(deadline);
    }

    pub(super) fn finish(self) -> thread::Result<()> {
        self.stop.store(true, Ordering::Release);
        self.handle.join()
    }
}

pub(super) fn spawn_mock_hub(mode: MockMode, artifact: Vec<u8>, race_directory: &Path) -> MockHub {
    server::spawn(mode, artifact, race_directory)
}
