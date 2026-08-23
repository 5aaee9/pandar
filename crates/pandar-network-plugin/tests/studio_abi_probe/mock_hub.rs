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
#[path = "mock_hub/stream.rs"]
mod stream;
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

pub(super) use stream::{Incoming, StreamUpgrade, snapshot_script};

pub(super) fn next_request(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    _method: &str,
    _path: &str,
) -> (TcpStream, String) {
    loop {
        match next_incoming(listener, stop, deadline) {
            Incoming::Stream(upgrade) => {
                let frames = upgrade.serve();
                for frame in responses::snapshot_frames(responses::PRINTERS_RESPONSE) {
                    frames
                        .send(frame)
                        .expect("serve incidental stream snapshot");
                }
            }
            Incoming::Http(stream, request) => return (stream, request),
        }
    }
}

pub(super) fn next_incoming(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
) -> Incoming {
    loop {
        let incoming = stream::next_incoming(listener, stop, deadline);
        if let Incoming::Http(mut stream, request) = incoming {
            if firmware_compat::try_respond(&mut stream, &request) {
                continue;
            }
            return Incoming::Http(stream, request);
        }
        return incoming;
    }
}

/// Blocks until a printer-events upgrade arrives, serving HTTP in between.
pub(super) fn next_stream(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
) -> StreamUpgrade {
    loop {
        match next_incoming(listener, stop, deadline) {
            Incoming::Stream(upgrade) => return upgrade,
            Incoming::Http(..) => {}
        }
    }
}

pub(super) enum MockMode {
    Success,
    ConnectionReadiness,
    BackgroundTimeout,
    StreamUnavailable,
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
    CameraAvailable,
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
