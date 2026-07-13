use anyhow::bail;

use super::*;

pub(crate) async fn assert_locked_for_a_moment<T, F>(
    gateway: &TestRuntimeBambuMachineGateway<T, F>,
) -> anyhow::Result<()>
where
    T: BambuMqttTransport + Send + Sync,
    F: MachineFileTransfer + Clone + Send + Sync,
{
    if gateway.inner.try_lock().is_ok() {
        bail!("runtime gateway lock was available while link_printer validation was paused");
    }
    Ok(())
}

pub(crate) async fn assert_unlocked_for_a_moment<T, F>(
    gateway: &TestRuntimeBambuMachineGateway<T, F>,
) -> anyhow::Result<()>
where
    T: BambuMqttTransport + Send + Sync,
    F: MachineFileTransfer + Clone + Send + Sync,
{
    let _inner = gateway
        .inner
        .try_lock()
        .map_err(|_| anyhow::anyhow!("runtime gateway lock was held during network validation"))?;
    Ok(())
}
