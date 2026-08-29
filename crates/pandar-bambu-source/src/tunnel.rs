use std::{
    io::Write,
    net::{Shutdown, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, TryRecvError, sync_channel},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use crate::{
    abi::{
        BAMBU_INVALID, BAMBU_STREAM_END, BAMBU_SUCCESS, BAMBU_WOULD_BLOCK, BambuFormat,
        BambuSample, BambuSessionStat, BambuStreamInfo, BambuVideoFormat, Logger,
        StreamInfoCallback, TrackReporter, VIDEO_JPEG, VIDEO_MJPG, VIDEO_STREAM,
    },
    config::RelayConfig,
    error::{SessionError, SessionTerminal, error_chain, set_last_error},
    reader::read_frames,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const MAX_FRAME_BYTES: usize = 10 * 1024 * 1024;
const FRAME_RATE: i32 = 5;
const FRAME_QUEUE_CAPACITY: usize = 4;
const LOG_ERROR: i32 = 3;

#[derive(Default)]
struct Callbacks {
    logger: Option<(Logger, usize)>,
    stream_info: Option<(StreamInfoCallback, usize)>,
    _track_reporter: Option<(TrackReporter, usize)>,
}

#[derive(Default)]
struct Stats {
    started: Option<Instant>,
    delivered_frames: u64,
    delivered_bytes: u64,
}

pub(crate) struct Shared {
    callbacks: Mutex<Callbacks>,
    dimensions: Mutex<Option<(i32, i32)>>,
    terminal: Mutex<Option<SessionTerminal>>,
    closing: AtomicBool,
    stats: Mutex<Stats>,
}

pub(crate) struct Tunnel {
    config: RelayConfig,
    shared: Arc<Shared>,
    receiver: Mutex<Option<Receiver<Vec<u8>>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    socket: Mutex<Option<TcpStream>>,
    current_sample: Mutex<Vec<u8>>,
    opened: AtomicBool,
}

impl Tunnel {
    pub(crate) fn new(config: RelayConfig) -> Self {
        Self {
            config,
            shared: Arc::new(Shared {
                callbacks: Mutex::new(Callbacks::default()),
                dimensions: Mutex::new(None),
                terminal: Mutex::new(None),
                closing: AtomicBool::new(false),
                stats: Mutex::new(Stats::default()),
            }),
            receiver: Mutex::new(None),
            worker: Mutex::new(None),
            socket: Mutex::new(None),
            current_sample: Mutex::new(Vec::new()),
            opened: AtomicBool::new(false),
        }
    }

    pub(crate) fn set_logger(&self, logger: Option<Logger>, context: *mut std::ffi::c_void) {
        self.shared
            .callbacks
            .lock()
            .expect("source callbacks")
            .logger = logger.map(|logger| (logger, context as usize));
    }

    pub(crate) fn set_stream_info_callback(
        &self,
        callback: Option<StreamInfoCallback>,
        context: *mut std::ffi::c_void,
    ) {
        self.shared
            .callbacks
            .lock()
            .expect("source callbacks")
            .stream_info = callback.map(|callback| (callback, context as usize));
    }

    pub(crate) fn set_track_reporter(
        &self,
        reporter: Option<TrackReporter>,
        context: *mut std::ffi::c_void,
    ) {
        self.shared
            .callbacks
            .lock()
            .expect("source callbacks")
            ._track_reporter = reporter.map(|reporter| (reporter, context as usize));
    }

    pub(crate) fn open(&self) -> i32 {
        if self.opened.swap(true, Ordering::AcqRel) {
            return BAMBU_SUCCESS;
        }
        self.shared.reset();
        match self.open_inner() {
            Ok(()) => BAMBU_SUCCESS,
            Err(error) => {
                self.opened.store(false, Ordering::Release);
                let message = error_chain(&error);
                self.shared.finish_failure(error);
                set_last_error(&message);
                BAMBU_INVALID
            }
        }
    }

    fn open_inner(&self) -> Result<(), SessionError> {
        let mut stream = TcpStream::connect_timeout(&self.config.address, CONNECT_TIMEOUT)
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
        send_relay_handshake(&mut stream, &self.config.auth)?;
        let control = stream
            .try_clone()
            .map_err(|error| SessionError::transport("cloning the relay socket", error))?;
        let (sender, receiver) = sync_channel(FRAME_QUEUE_CAPACITY);
        *self.receiver.lock().expect("source receiver") = Some(receiver);
        *self.socket.lock().expect("source socket") = Some(control);
        *self.shared.stats.lock().expect("source stats") = Stats {
            started: Some(Instant::now()),
            ..Stats::default()
        };
        let shared = Arc::clone(&self.shared);
        *self.worker.lock().expect("source worker") = Some(std::thread::spawn(move || {
            read_frames(stream, sender, &shared);
        }));
        Ok(())
    }

    pub(crate) fn start_stream(&self, video: bool) -> i32 {
        if !video || !self.opened.load(Ordering::Acquire) {
            return BAMBU_INVALID;
        }
        if self
            .shared
            .dimensions
            .lock()
            .expect("source dimensions")
            .is_some()
        {
            BAMBU_SUCCESS
        } else {
            self.shared.pending_code()
        }
    }

    pub(crate) fn stream_info(&self, index: i32, output: *mut BambuStreamInfo) -> i32 {
        if index != 0 || output.is_null() {
            return BAMBU_INVALID;
        }
        let Some((width, height)) = *self.shared.dimensions.lock().expect("source dimensions")
        else {
            return self.shared.pending_code();
        };
        unsafe { output.write(stream_info(width, height)) };
        BAMBU_SUCCESS
    }

    pub(crate) fn read_sample(&self, output: *mut BambuSample) -> i32 {
        if output.is_null() {
            return BAMBU_INVALID;
        }
        let receiver = self.receiver.lock().expect("source receiver");
        let Some(receiver) = receiver.as_ref() else {
            return BAMBU_INVALID;
        };
        match receiver.try_recv() {
            Ok(frame) => {
                let mut current = self.current_sample.lock().expect("source current sample");
                *current = frame;
                let Ok(size) = i32::try_from(current.len()) else {
                    return BAMBU_INVALID;
                };
                unsafe {
                    output.write(BambuSample {
                        itrack: 0,
                        size,
                        flags: 1,
                        buffer: current.as_ptr(),
                        decode_time: 0,
                    });
                }
                let mut stats = self.shared.stats.lock().expect("source stats");
                stats.delivered_frames += 1;
                stats.delivered_bytes += current.len() as u64;
                BAMBU_SUCCESS
            }
            Err(TryRecvError::Empty) => self.shared.pending_code(),
            Err(TryRecvError::Disconnected) => self.shared.terminal_code(),
        }
    }

    pub(crate) fn session_stat(&self, output: *mut BambuSessionStat) {
        if output.is_null() {
            return;
        }
        let stats = self.shared.stats.lock().expect("source stats");
        let duration = stats
            .started
            .map_or(Duration::ZERO, |started| started.elapsed());
        let seconds = duration.as_secs_f32();
        let avg_fps = if seconds > 0.0 {
            stats.delivered_frames as f32 / seconds
        } else {
            0.0
        };
        let avg_bitrate_kbps = if seconds > 0.0 {
            stats.delivered_bytes as f32 * 8.0 / seconds / 1000.0
        } else {
            0.0
        };
        unsafe {
            output.write(BambuSessionStat {
                session_duration_ms: duration.as_millis().min(i64::MAX as u128) as i64,
                freeze_total_duration_ms: 0,
                freeze_count: 0,
                avg_fps,
                avg_bitrate_kbps,
                avg_jitter_ms: 0.0,
                max_jitter_ms: 0.0,
            });
        }
    }

    pub(crate) fn close(&self) {
        self.opened.store(false, Ordering::Release);
        self.shared.closing.store(true, Ordering::Release);
        if let Some(stream) = self.socket.lock().expect("source socket").take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        self.receiver.lock().expect("source receiver").take();
        if let Some(worker) = self.worker.lock().expect("source worker").take() {
            let _ = worker.join();
        }
        self.shared.finish_eof();
    }
}

impl Shared {
    fn reset(&self) {
        self.closing.store(false, Ordering::Release);
        *self.terminal.lock().expect("source terminal") = None;
        *self.dimensions.lock().expect("source dimensions") = None;
    }

    fn pending_code(&self) -> i32 {
        if self.terminal.lock().expect("source terminal").is_none() {
            BAMBU_WOULD_BLOCK
        } else {
            self.terminal_code()
        }
    }

    fn terminal_code(&self) -> i32 {
        match self.terminal.lock().expect("source terminal").as_ref() {
            Some(SessionTerminal::Failure(error)) => {
                set_last_error(&error_chain(error));
                BAMBU_INVALID
            }
            Some(SessionTerminal::Eof) | None => BAMBU_STREAM_END,
        }
    }

    pub(crate) fn finish_eof(&self) {
        let mut terminal = self.terminal.lock().expect("source terminal");
        if terminal.is_none() {
            *terminal = Some(SessionTerminal::Eof);
        }
    }

    pub(crate) fn finish_failure(&self, error: SessionError) {
        if self.closing.load(Ordering::Acquire) {
            self.finish_eof();
            return;
        }
        let message = error_chain(&error);
        let mut terminal = self.terminal.lock().expect("source terminal");
        if terminal.is_some() {
            return;
        }
        *terminal = Some(SessionTerminal::Failure(error));
        drop(terminal);
        self.log_error(&message);
    }

    fn log_error(&self, message: &str) {
        let callbacks = self.callbacks.lock().expect("source callbacks");
        if let Some((logger, context)) = callbacks.logger {
            log_message(logger, context, message);
        }
    }

    pub(crate) fn publish_dimensions(&self, width: i32, height: i32) {
        let changed = {
            let mut dimensions = self.dimensions.lock().expect("source dimensions");
            if dimensions.as_ref() == Some(&(width, height)) {
                false
            } else {
                *dimensions = Some((width, height));
                true
            }
        };
        if changed {
            let callbacks = self.callbacks.lock().expect("source callbacks");
            if let Some((callback, context)) = callbacks.stream_info {
                let mut info = stream_info(width, height);
                unsafe { callback(context as *mut std::ffi::c_void, &mut info) };
            }
        }
    }
}

pub(crate) fn send_relay_handshake<W: Write>(
    writer: &mut W,
    auth: &[u8; 32],
) -> Result<(), SessionError> {
    writer.write_all(auth).map_err(SessionError::Handshake)
}

#[cfg(not(target_os = "windows"))]
fn log_message(logger: Logger, context: usize, message: &str) {
    let message = std::ffi::CString::new(message)
        .expect("session errors contain no NUL bytes")
        .into_raw();
    unsafe { logger(context as *mut std::ffi::c_void, LOG_ERROR, message) };
}

#[cfg(target_os = "windows")]
fn log_message(logger: Logger, context: usize, message: &str) {
    let message = message
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let message = Box::into_raw(message).cast::<u16>();
    unsafe { logger(context as *mut std::ffi::c_void, LOG_ERROR, message) };
}

fn stream_info(width: i32, height: i32) -> BambuStreamInfo {
    BambuStreamInfo {
        stream_type: VIDEO_STREAM,
        sub_type: VIDEO_MJPG,
        format: BambuFormat {
            video: BambuVideoFormat {
                width,
                height,
                frame_rate: FRAME_RATE,
            },
        },
        format_type: VIDEO_JPEG,
        format_size: 0,
        max_frame_size: MAX_FRAME_BYTES as i32,
        format_buffer: std::ptr::null(),
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.close();
    }
}
