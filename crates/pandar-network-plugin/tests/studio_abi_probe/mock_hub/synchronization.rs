use std::{
    sync::mpsc::{Receiver, SyncSender, sync_channel},
    time::Instant,
};

pub(super) struct ProbeStart {
    sender: SyncSender<Instant>,
}

pub(super) struct HubStart {
    receiver: Receiver<Instant>,
}

pub(super) fn start_gate() -> (ProbeStart, HubStart) {
    let (sender, receiver) = sync_channel(0);
    (ProbeStart { sender }, HubStart { receiver })
}

impl ProbeStart {
    pub(super) fn arm(&self, deadline: Instant) {
        self.sender
            .send(deadline)
            .expect("start mock Hub after Studio ABI probe spawn");
    }
}

impl HubStart {
    pub(super) fn wait(self) -> Instant {
        self.receiver
            .recv()
            .expect("Studio ABI probe exited before starting the mock Hub")
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write,
        net::{TcpListener, TcpStream},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc::{RecvTimeoutError, sync_channel},
        },
        thread,
        time::{Duration, Instant},
    };

    use super::start_gate;
    use crate::mock_hub::transport::read_request_until;

    const EVENT_TIMEOUT: Duration = Duration::from_secs(2);

    #[test]
    fn request_deadline_starts_only_after_probe_is_armed() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let (probe_start, hub_start) = start_gate();
        let (waiting, wait_started) = sync_channel(0);
        let (result, request_finished) = sync_channel(0);

        let handle = thread::spawn(move || {
            waiting.send(()).unwrap();
            let deadline = hub_start.wait();
            let request = read_request_until(&listener, &thread_stop, deadline, "test request");
            result.send(request.is_some()).unwrap();
        });

        wait_started.recv_timeout(EVENT_TIMEOUT).unwrap();
        assert_eq!(
            request_finished.recv_timeout(Duration::from_millis(25)),
            Err(RecvTimeoutError::Timeout)
        );

        probe_start.arm(Instant::now() + EVENT_TIMEOUT);
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .write_all(b"GET /ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        assert!(request_finished.recv_timeout(EVENT_TIMEOUT).unwrap());
        handle.join().unwrap();
    }

    #[test]
    fn armed_accept_stops_promptly_without_a_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let (probe_start, hub_start) = start_gate();
        let (accepting, accept_started) = sync_channel(0);
        let (result, accept_finished) = sync_channel(0);

        let handle = thread::spawn(move || {
            let deadline = hub_start.wait();
            accepting.send(()).unwrap();
            let request = read_request_until(&listener, &thread_stop, deadline, "test request");
            result.send(request.is_none()).unwrap();
        });

        probe_start.arm(Instant::now() + EVENT_TIMEOUT);
        accept_started.recv_timeout(EVENT_TIMEOUT).unwrap();
        stop.store(true, Ordering::Release);
        assert!(
            accept_finished
                .recv_timeout(Duration::from_millis(250))
                .unwrap()
        );
        handle.join().unwrap();
    }
}
