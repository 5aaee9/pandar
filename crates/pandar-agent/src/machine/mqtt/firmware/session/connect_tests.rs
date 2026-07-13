use super::connect::connect_failure_with_cleanup;
use anyhow::anyhow;

#[tokio::test]
async fn timeout_failure_preserves_completed_pump_join_error() {
    let task = tokio::spawn(async {
        panic!("completed timeout pump panic sentinel");
    });
    let join_error = task.await.unwrap_err();

    let error = connect_failure_with_cleanup(
        anyhow!("timed out waiting for firmware MQTT SUBACK"),
        Err(join_error),
    );
    let message = format!("{error:#}");

    assert!(message.contains("timed out waiting for firmware MQTT SUBACK"));
    assert!(message.contains("completed timeout pump panic sentinel"));
}
