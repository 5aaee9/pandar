use super::*;
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};
use tracing_subscriber::fmt::MakeWriter;

#[tokio::test]
async fn link_printer_dispatch_failure_helper_redacts_access_code_in_logs() {
    let logs = CapturedLogs::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.writer())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let access_code = "SECRET-LINK-CODE";
    let payload = LinkPrinterPayload {
        printer_type: "BambuLab".to_owned(),
        host: "192.0.2.10".to_owned(),
        access_code: access_code.to_owned(),
        name: None,
    };

    let err = fail_link_printer_dispatch_after_commit(
        CommandId::new(),
        TenantId::new(),
        AgentId::new(),
        &payload,
        "agent connection closed before printer link completed".to_owned(),
        |_command_id, _tenant_id, _agent_id, _error| async move {
            Err(crate::repositories::RepositoryError::Database(
                anyhow::anyhow!("failed while handling access_code=SECRET-LINK-CODE"),
            ))
        },
    )
    .await
    .unwrap_err();
    drop(_guard);

    assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(err.code, "internal_server_error");
    assert!(!logs.to_string().contains(access_code));
}

#[derive(Clone)]
struct CapturedLogs {
    output: Arc<Mutex<Vec<u8>>>,
}

impl CapturedLogs {
    fn new() -> Self {
        Self {
            output: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn writer(&self) -> TestLogWriter {
        TestLogWriter {
            output: self.output.clone(),
        }
    }
}

impl std::fmt::Display for CapturedLogs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let output = self.output.lock().unwrap().clone();
        formatter.write_str(&String::from_utf8_lossy(&output))
    }
}

#[derive(Clone)]
struct TestLogWriter {
    output: Arc<Mutex<Vec<u8>>>,
}

impl<'writer> MakeWriter<'writer> for TestLogWriter {
    type Writer = TestLogBuffer;

    fn make_writer(&'writer self) -> Self::Writer {
        TestLogBuffer {
            output: self.output.clone(),
        }
    }
}

struct TestLogBuffer {
    output: Arc<Mutex<Vec<u8>>>,
}

impl Write for TestLogBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
