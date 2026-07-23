use std::{
    fs,
    io::Write,
    net::{SocketAddr, TcpListener, TcpStream},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
};

use super::super::compiler::{build_plugin, compile_firmware_snapshot_claim_probe};

struct ZeroIoHub {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    requests: Arc<AtomicUsize>,
    thread: thread::JoinHandle<()>,
}

impl ZeroIoHub {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind zero-I/O Hub");
        let address = listener.local_addr().expect("read zero-I/O Hub address");
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(AtomicUsize::new(0));
        let thread_stop = Arc::clone(&stop);
        let thread_requests = Arc::clone(&requests);
        let thread = thread::spawn(move || {
            loop {
                let (mut stream, _) = listener.accept().expect("accept zero-I/O Hub request");
                if thread_stop.load(Ordering::Acquire) {
                    return;
                }
                thread_requests.fetch_add(1, Ordering::AcqRel);
                stream
                    .write_all(
                        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                    )
                    .expect("respond to forbidden firmware request");
            }
        });
        Self {
            address,
            stop,
            requests,
            thread,
        }
    }

    fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    fn finish(self) -> usize {
        self.stop.store(true, Ordering::Release);
        TcpStream::connect(self.address).expect("wake zero-I/O Hub");
        self.thread.join().expect("join zero-I/O Hub");
        self.requests.load(Ordering::Acquire)
    }
}

#[test]
fn compiled_snapshot_generation_claim_rejects_all_stale_firmware_requests() {
    let compiled = compile_firmware_snapshot_claim_probe();
    let built_library = build_plugin();
    let run_directory = tempfile::tempdir().expect("create firmware claim run directory");
    let library = run_directory.path().join(
        built_library
            .file_name()
            .expect("built Studio plugin library has a file name"),
    );
    fs::copy(&built_library, &library).expect("copy Studio plugin library");
    let hub_a = ZeroIoHub::spawn();
    let hub_b = ZeroIoHub::spawn();
    let output = Command::new(&compiled.executable)
        .arg(library)
        .arg(hub_a.url())
        .arg(hub_b.url())
        .current_dir(run_directory.path())
        .output()
        .expect("run firmware snapshot claim probe");
    let requests_a = hub_a.finish();
    let requests_b = hub_b.finish();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "firmware snapshot claim probe failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(stdout.trim(), r#"{"ok":true,"send_token":0}"#);
    assert_eq!(requests_a, 0, "generation A Hub received stale request");
    assert_eq!(
        requests_b, 0,
        "generation B Hub received generation A request"
    );
}
