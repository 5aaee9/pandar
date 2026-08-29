use std::io;

use tokio::sync::mpsc;

use super::pump::{FirmwareMqttOperationPhase, attempt_pump_failure, pump_failure};

#[test]
fn send_failure_preserves_phase_and_source_across_thread_boundary() {
    assert_transport_preserves_cause(
        FirmwareMqttOperationPhase::Send,
        false,
        "queue firmware send",
        "firmware send cause sentinel",
    );
}

#[test]
fn receive_failure_preserves_phase_and_source_across_thread_boundary() {
    assert_transport_preserves_cause(
        FirmwareMqttOperationPhase::Receive,
        true,
        "decode firmware response",
        "firmware receive cause sentinel",
    );
}

fn assert_transport_preserves_cause(
    phase: FirmwareMqttOperationPhase,
    after_publish: bool,
    context: &'static str,
    cause: &'static str,
) {
    let (sender, mut receiver) = mpsc::unbounded_channel();
    std::thread::spawn(move || {
        let source = anyhow::Error::new(io::Error::other(cause)).context(context);
        sender
            .send(pump_failure(phase, source))
            .expect("failure receiver remains open");
    })
    .join()
    .unwrap();
    let failure = receiver.blocking_recv().unwrap();

    let error = attempt_pump_failure(after_publish, failure);
    let message = format!("{error:#}");
    assert!(message.contains(&format!("firmware MQTT {phase} operation failed")));
    assert!(message.contains(context));
    assert!(message.contains(cause));
}
