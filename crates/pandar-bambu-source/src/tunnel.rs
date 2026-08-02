use std::{
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, TryRecvError, sync_channel},
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
    config::{RelayConfig, jpeg_dimensions},
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_FRAME_BYTES: usize = 10 * 1024 * 1024;
const FRAME_RATE: i32 = 5;
const FRAME_QUEUE_CAPACITY: usize = 4;

#[derive(Default)]
struct Callbacks {
    _logger: Option<(Logger, usize)>,
    stream_info: Option<(StreamInfoCallback, usize)>,
    _track_reporter: Option<(TrackReporter, usize)>,
}

#[derive(Default)]
struct Stats {
    started: Option<Instant>,
    delivered_frames: u64,
    delivered_bytes: u64,
}

struct Shared {
    callbacks: Mutex<Callbacks>,
    dimensions: Mutex<Option<(i32, i32)>>,
    ended: AtomicBool,
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
                ended: AtomicBool::new(false),
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
            ._logger = logger.map(|logger| (logger, context as usize));
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
        match self.open_inner() {
            Ok(()) => BAMBU_SUCCESS,
            Err(()) => {
                self.opened.store(false, Ordering::Release);
                BAMBU_INVALID
            }
        }
    }

    fn open_inner(&self) -> Result<(), ()> {
        let mut stream =
            TcpStream::connect_timeout(&self.config.address, CONNECT_TIMEOUT).map_err(|_| ())?;
        stream.set_nodelay(true).map_err(|_| ())?;
        stream
            .set_read_timeout(Some(READ_TIMEOUT))
            .map_err(|_| ())?;
        stream
            .set_write_timeout(Some(READ_TIMEOUT))
            .map_err(|_| ())?;
        stream.write_all(&self.config.auth).map_err(|_| ())?;
        let control = stream.try_clone().map_err(|_| ())?;
        let (sender, receiver) = sync_channel(FRAME_QUEUE_CAPACITY);
        *self.receiver.lock().expect("source receiver") = Some(receiver);
        *self.socket.lock().expect("source socket") = Some(control);
        self.shared.ended.store(false, Ordering::Release);
        *self.shared.dimensions.lock().expect("source dimensions") = None;
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
        } else if self.shared.ended.load(Ordering::Acquire) {
            BAMBU_STREAM_END
        } else {
            BAMBU_WOULD_BLOCK
        }
    }

    pub(crate) fn stream_info(&self, index: i32, output: *mut BambuStreamInfo) -> i32 {
        if index != 0 || output.is_null() {
            return BAMBU_INVALID;
        }
        let Some((width, height)) = *self.shared.dimensions.lock().expect("source dimensions")
        else {
            return if self.shared.ended.load(Ordering::Acquire) {
                BAMBU_STREAM_END
            } else {
                BAMBU_WOULD_BLOCK
            };
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
            Err(TryRecvError::Empty) if !self.shared.ended.load(Ordering::Acquire) => {
                BAMBU_WOULD_BLOCK
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => BAMBU_STREAM_END,
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
        if let Some(stream) = self.socket.lock().expect("source socket").take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        self.receiver.lock().expect("source receiver").take();
        if let Some(worker) = self.worker.lock().expect("source worker").take() {
            let _ = worker.join();
        }
        self.shared.ended.store(true, Ordering::Release);
    }
}

fn read_frames(mut stream: TcpStream, sender: SyncSender<Vec<u8>>, shared: &Shared) {
    loop {
        let mut length = [0_u8; 4];
        if stream.read_exact(&mut length).is_err() {
            break;
        }
        let length = u32::from_le_bytes(length) as usize;
        if length == 0 || length > MAX_FRAME_BYTES {
            break;
        }
        let mut frame = vec![0_u8; length];
        if stream.read_exact(&mut frame).is_err() {
            break;
        }
        let Some((width, height)) = jpeg_dimensions(&frame) else {
            break;
        };
        publish_dimensions(shared, width, height);
        match sender.try_send(frame) {
            Ok(()) | Err(std::sync::mpsc::TrySendError::Full(_)) => {}
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => break,
        }
    }
    shared.ended.store(true, Ordering::Release);
}

fn publish_dimensions(shared: &Shared, width: i32, height: i32) {
    let changed = {
        let mut dimensions = shared.dimensions.lock().expect("source dimensions");
        if dimensions.as_ref() == Some(&(width, height)) {
            false
        } else {
            *dimensions = Some((width, height));
            true
        }
    };
    let callback = changed
        .then(|| {
            shared
                .callbacks
                .lock()
                .expect("source callbacks")
                .stream_info
        })
        .flatten();
    if let Some((callback, context)) = callback {
        let mut info = stream_info(width, height);
        unsafe { callback(context as *mut std::ffi::c_void, &mut info) };
    }
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
