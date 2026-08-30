use std::{
    net::{Shutdown, TcpStream},
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{Receiver, TryRecvError, sync_channel},
    },
    thread::JoinHandle,
    time::Instant,
};

use crate::{
    abi::{BAMBU_INVALID, BAMBU_SUCCESS},
    config::RelayConfig,
    error::{SessionError, error_chain, set_last_error},
    reader::read_frames,
};

use super::{FRAME_QUEUE_CAPACITY, READ_TIMEOUT, Shared, Stats, send_relay_handshake};

pub(super) struct TunnelLifecycle {
    state: Mutex<State>,
    changed: Condvar,
    #[cfg(test)]
    pause: Mutex<Option<Arc<LifecycleTestPause>>>,
}

enum State {
    Closed,
    Running(Session),
    Closing(std::thread::ThreadId),
}

struct Session {
    receiver: Receiver<Vec<u8>>,
    worker: JoinHandle<()>,
    socket: TcpStream,
}

impl TunnelLifecycle {
    pub(super) fn new() -> Self {
        Self {
            state: Mutex::new(State::Closed),
            changed: Condvar::new(),
            #[cfg(test)]
            pause: Mutex::new(None),
        }
    }

    pub(super) fn open(&self, config: &RelayConfig, shared: &Arc<Shared>) -> i32 {
        let mut state = self.state.lock().expect("source lifecycle");
        loop {
            match &*state {
                State::Running(_) => return BAMBU_SUCCESS,
                State::Closed => break,
                State::Closing(_) => {
                    state = self.changed.wait(state).expect("source lifecycle");
                }
            }
        }
        #[cfg(test)]
        self.pause_open();
        shared.reset();
        match Session::connect(config, shared) {
            Ok(session) => {
                *state = State::Running(session);
                BAMBU_SUCCESS
            }
            Err(error) => {
                let message = error_chain(&error);
                let should_log = shared.record_failure(error);
                drop(state);
                if should_log {
                    shared.log_error(&message);
                }
                set_last_error(&message);
                BAMBU_INVALID
            }
        }
    }

    pub(super) fn is_running(&self) -> bool {
        matches!(
            *self.state.lock().expect("source lifecycle"),
            State::Running(_)
        )
    }

    pub(super) fn try_read(&self) -> Option<Result<Vec<u8>, TryRecvError>> {
        let state = self.state.lock().expect("source lifecycle");
        let State::Running(session) = &*state else {
            return None;
        };
        Some(session.receiver.try_recv())
    }

    pub(super) fn close(&self, shared: &Shared) {
        #[cfg(test)]
        self.pause_close();
        let session = {
            let mut state = self.state.lock().expect("source lifecycle");
            loop {
                match &*state {
                    State::Closed => {
                        shared.finish_eof();
                        return;
                    }
                    State::Closing(worker_id) => {
                        if worker_id == &std::thread::current().id() {
                            return;
                        }
                        state = self.changed.wait(state).expect("source lifecycle");
                    }
                    State::Running(session) => {
                        shared
                            .closing
                            .store(true, std::sync::atomic::Ordering::Release);
                        let worker_id = session.worker.thread().id();
                        let State::Running(session) =
                            std::mem::replace(&mut *state, State::Closing(worker_id))
                        else {
                            unreachable!("running lifecycle state was just matched")
                        };
                        break session;
                    }
                }
            }
        };
        let Session {
            receiver,
            worker,
            socket,
        } = session;
        let _ = socket.shutdown(Shutdown::Both);
        drop(receiver);
        let _ = worker.join();
        shared.finish_eof();
        *self.state.lock().expect("source lifecycle") = State::Closed;
        self.changed.notify_all();
    }

    #[cfg(test)]
    fn set_pause(&self, pause: Option<Arc<LifecycleTestPause>>) {
        *self.pause.lock().expect("source lifecycle pause") = pause;
    }

    #[cfg(test)]
    fn pause_open(&self) {
        let pause = self.pause.lock().expect("source lifecycle pause").clone();
        if let Some(pause) = pause {
            pause.open_reached.wait();
            pause.open_release.wait();
        }
    }

    #[cfg(test)]
    fn pause_close(&self) {
        let pause = self.pause.lock().expect("source lifecycle pause").clone();
        if let Some(pause) = pause {
            pause.close_reached.wait();
        }
    }
}

impl Session {
    fn connect(config: &RelayConfig, shared: &Arc<Shared>) -> Result<Self, SessionError> {
        let mut stream = TcpStream::connect_timeout(&config.address, super::CONNECT_TIMEOUT)
            .map_err(|error| SessionError::transport("connecting to the loopback relay", error))?;
        stream
            .set_nodelay(true)
            .map_err(|error| SessionError::transport("configuring TCP_NODELAY", error))?;
        stream
            .set_read_timeout(Some(READ_TIMEOUT))
            .map_err(|error| SessionError::transport("configuring the read timeout", error))?;
        stream
            .set_write_timeout(Some(READ_TIMEOUT))
            .map_err(|error| SessionError::transport("configuring the write timeout", error))?;
        send_relay_handshake(&mut stream, &config.auth)?;
        let socket = stream
            .try_clone()
            .map_err(|error| SessionError::transport("cloning the relay socket", error))?;
        let (sender, receiver) = sync_channel(FRAME_QUEUE_CAPACITY);
        *shared.stats.lock().expect("source stats") = Stats {
            started: Some(Instant::now()),
            ..Stats::default()
        };
        let shared = Arc::clone(shared);
        let worker = std::thread::spawn(move || read_frames(stream, sender, &shared));
        Ok(Self {
            receiver,
            worker,
            socket,
        })
    }
}

#[cfg(test)]
struct LifecycleTestPause {
    open_reached: std::sync::Barrier,
    open_release: std::sync::Barrier,
    close_reached: std::sync::Barrier,
}

#[cfg(test)]
impl LifecycleTestPause {
    fn new() -> Self {
        Self {
            open_reached: std::sync::Barrier::new(2),
            open_release: std::sync::Barrier::new(2),
            close_reached: std::sync::Barrier::new(2),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Read, net::TcpListener, sync::Arc, time::Duration};

    use crate::{
        abi::{BAMBU_INVALID, BAMBU_SUCCESS},
        config::RelayConfig,
        tunnel::Tunnel,
    };

    use super::LifecycleTestPause;

    #[test]
    fn close_during_open_tears_down_the_committed_session() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let auth = *b"0123456789abcdef0123456789abcdef";
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut presented = [0_u8; 32];
            stream.read_exact(&mut presented).unwrap();
            assert_eq!(presented, auth);
            let mut trailing = [0_u8; 1];
            assert_eq!(stream.read(&mut trailing).unwrap(), 0);
        });
        let tunnel = Arc::new(Tunnel::new(RelayConfig { address, auth }));
        let pause = Arc::new(LifecycleTestPause::new());
        tunnel.lifecycle.set_pause(Some(Arc::clone(&pause)));

        let opening_tunnel = Arc::clone(&tunnel);
        let opening = std::thread::spawn(move || opening_tunnel.open());
        pause.open_reached.wait();

        let closing_tunnel = Arc::clone(&tunnel);
        let closing = std::thread::spawn(move || closing_tunnel.close());
        pause.close_reached.wait();
        pause.open_release.wait();

        assert_eq!(opening.join().unwrap(), BAMBU_SUCCESS);
        closing.join().unwrap();
        tunnel.lifecycle.set_pause(None);
        assert_eq!(tunnel.start_stream(true), BAMBU_INVALID);
        server.join().unwrap();
    }
}
