use anyhow::{Context, anyhow, bail};
use pandar_core::{BambuDeviceFeature, BambuDeviceFeatures};
use tokio::sync::mpsc;

use super::PrinterAxis;
use crate::{
    AgentConfig,
    machine::{
        ConfiguredBambuMachineGateway, DeviceFeatureCache, PrinterOperation,
        PrinterOperationDispatchResult,
        mqtt::{BambuMqttCommand, BambuMqttTransport, GcodeLineCommand, feature_event},
    },
};
use pandar_protocol::agent::v1::AgentEvent;

pub(crate) async fn operate_printer_with_feature_selection<T, F>(
    config: &AgentConfig,
    inner: &tokio::sync::Mutex<ConfiguredBambuMachineGateway<T, F>>,
    device_features: &DeviceFeatureCache,
    current_sender: &tokio::sync::Mutex<Option<mpsc::Sender<AgentEvent>>>,
    serial_number: &str,
    operation: PrinterOperation,
) -> anyhow::Result<PrinterOperationDispatchResult>
where
    T: BambuMqttTransport + Send + Sync,
{
    operation.validate().map_err(anyhow::Error::new)?;
    let Some(required_feature) = operation
        .required_device_features()
        .first()
        .copied()
        .map(pandar_core::RequiredDeviceFeature::bambu_feature)
    else {
        return inner
            .lock()
            .await
            .operate_printer_with_device_feature_lease(serial_number, operation, None)
            .await;
    };
    let sender = current_sender
        .lock()
        .await
        .clone()
        .ok_or_else(|| anyhow!("no current Agent event sender for printer operation"))?;
    let cached = device_features.get(serial_number).await;
    let observed = match cached {
        Some(value) => value,
        None => {
            let result = inner
                .lock()
                .await
                .probe_device_features(serial_number, device_features)
                .await
                .with_context(|| {
                    format!(
                        "probe printer {serial_number} for required device feature bit {}",
                        required_feature.bit()
                    )
                });
            match result {
                Ok(value) => {
                    queue_feature_convergence(config, &sender, serial_number, Some(value)).await?;
                    value
                }
                Err(error) => {
                    device_features.invalidate(serial_number).await;
                    if let Err(event_error) =
                        queue_feature_convergence(config, &sender, serial_number, None).await
                    {
                        return Err(error.context(format!(
                            "queue printer device feature invalidation also failed: {event_error:#}"
                        )));
                    }
                    return Err(error);
                }
            }
        }
    };
    if !observed.contains(required_feature) {
        if cached.is_some() {
            queue_feature_convergence(config, &sender, serial_number, Some(observed)).await?;
        }
        bail!(
            "printer {serial_number} device feature bitmap {observed} is missing required feature bit {}",
            required_feature.bit()
        );
    }

    let gateway = inner.lock().await;
    #[cfg(test)]
    pause::wait(serial_number, pause::Phase::BeforeFinalLease).await;
    let lease = device_features.transition_lease(serial_number).await;
    let current = lease.get();
    if current.is_none_or(|features| !features.contains(required_feature)) {
        drop(lease);
        drop(gateway);
        queue_feature_convergence(config, &sender, serial_number, current).await?;
        match current {
            Some(features) => bail!(
                "printer {serial_number} device feature bitmap {features} is missing required feature bit {}",
                required_feature.bit()
            ),
            None => bail!(
                "printer {serial_number} device feature bitmap is unknown before required feature bit {} publish",
                required_feature.bit()
            ),
        }
    }
    #[cfg(test)]
    pause::wait(serial_number, pause::Phase::AfterFinalReadBeforePublish).await;
    let outcome = gateway
        .operate_printer_with_device_feature_lease(serial_number, operation, Some(lease))
        .await;
    if let Err(error) = outcome {
        let current = device_features.get(serial_number).await;
        if current.is_none_or(|features| !features.contains(required_feature))
            && let Err(event_error) =
                queue_feature_convergence(config, &sender, serial_number, current).await
        {
            return Err(error.context(format!(
                "queue printer device feature convergence event also failed: {event_error:#}"
            )));
        }
        return Err(error);
    }
    outcome
}

async fn queue_feature_convergence(
    config: &AgentConfig,
    sender: &mpsc::Sender<AgentEvent>,
    serial_number: &str,
    value: Option<BambuDeviceFeatures>,
) -> anyhow::Result<()> {
    sender
        .send(feature_event(config, serial_number.to_owned(), value))
        .await
        .context("queue printer device feature convergence event")
}

pub(super) fn home_command(
    axes: Vec<PrinterAxis>,
    required_feature: Option<BambuDeviceFeature>,
    observed_features: Option<BambuDeviceFeatures>,
) -> anyhow::Result<BambuMqttCommand> {
    match required_feature {
        None => Ok(BambuMqttCommand::GcodeLine(GcodeLineCommand {
            param: home_gcode_line(&axes),
        })),
        Some(BambuDeviceFeature::MqttHoming) if axes.is_empty() => {
            require_observed_feature(observed_features, BambuDeviceFeature::MqttHoming)?;
            Ok(BambuMqttCommand::BackToCenter)
        }
        Some(feature) => bail!(
            "invalid modern home semantics for required device feature bit {}",
            feature.bit()
        ),
    }
}

pub(super) fn move_axes_command(
    x_mm: Option<f64>,
    y_mm: Option<f64>,
    z_mm: Option<f64>,
    feedrate_mm_per_min: Option<f64>,
    required_feature: Option<BambuDeviceFeature>,
    observed_features: Option<BambuDeviceFeatures>,
) -> anyhow::Result<BambuMqttCommand> {
    match required_feature {
        None => Ok(BambuMqttCommand::GcodeLine(GcodeLineCommand {
            param: legacy_move_lines(x_mm, y_mm, z_mm, feedrate_mm_per_min).join("\n"),
        })),
        Some(BambuDeviceFeature::MqttAxisControl) => {
            let (axis, delta) = modern_axis_move(x_mm, y_mm, z_mm, feedrate_mm_per_min)?;
            require_observed_feature(observed_features, BambuDeviceFeature::MqttAxisControl)?;
            Ok(BambuMqttCommand::XyzControl {
                axis,
                direction: if delta.is_sign_positive() { 1 } else { -1 },
                mode: if delta.abs() == 1.0 { 0 } else { 1 },
            })
        }
        Some(feature) => bail!(
            "invalid modern axis movement semantics for required device feature bit {}",
            feature.bit()
        ),
    }
}

fn require_observed_feature(
    observed_features: Option<BambuDeviceFeatures>,
    required_feature: BambuDeviceFeature,
) -> anyhow::Result<()> {
    match observed_features {
        Some(features) if features.contains(required_feature) => Ok(()),
        Some(features) => bail!(
            "device feature bitmap {features} is missing required feature bit {}",
            required_feature.bit()
        ),
        None => bail!(
            "device feature observation is unavailable for required feature bit {}",
            required_feature.bit()
        ),
    }
}

fn modern_axis_move(
    x_mm: Option<f64>,
    y_mm: Option<f64>,
    z_mm: Option<f64>,
    feedrate_mm_per_min: Option<f64>,
) -> anyhow::Result<(PrinterAxis, f64)> {
    let axes = [
        (PrinterAxis::X, x_mm),
        (PrinterAxis::Y, y_mm),
        (PrinterAxis::Z, z_mm),
    ];
    let mut movements = axes
        .into_iter()
        .filter_map(|(axis, delta)| delta.map(|delta| (axis, delta)));
    let movement = movements.next();
    if feedrate_mm_per_min.is_some()
        || movements.next().is_some()
        || !movement.is_some_and(|(_, delta)| matches!(delta.abs(), 1.0 | 10.0))
    {
        bail!("invalid modern axis movement; expected one axis by 1mm or 10mm without feedrate");
    }
    Ok(movement.expect("validated modern axis movement"))
}

fn home_gcode_line(axes: &[PrinterAxis]) -> String {
    let mut line = String::from("G28");
    for axis in axes {
        line.push(' ');
        line.push_str(axis_name(*axis));
    }
    line
}

fn legacy_move_lines(
    x_mm: Option<f64>,
    y_mm: Option<f64>,
    z_mm: Option<f64>,
    feedrate_mm_per_min: Option<f64>,
) -> Vec<String> {
    vec![
        "M211 S".to_owned(),
        "M211 X1 Y1 Z1".to_owned(),
        "M1002 push_ref_mode".to_owned(),
        "G91".to_owned(),
        move_gcode_line(x_mm, y_mm, z_mm, feedrate_mm_per_min),
        "M1002 pop_ref_mode".to_owned(),
        "M211 R".to_owned(),
    ]
}

fn move_gcode_line(
    x_mm: Option<f64>,
    y_mm: Option<f64>,
    z_mm: Option<f64>,
    feedrate_mm_per_min: Option<f64>,
) -> String {
    let mut line = String::from("G1");
    for (axis, value) in [("X", x_mm), ("Y", y_mm), ("Z", z_mm)] {
        if let Some(value) = value {
            line.push_str(&format!(" {axis}{}", format_gcode_number(value)));
        }
    }
    if let Some(value) = feedrate_mm_per_min {
        line.push_str(&format!(" F{}", format_gcode_number(value)));
    }
    line
}

fn axis_name(axis: PrinterAxis) -> &'static str {
    match axis {
        PrinterAxis::X => "X",
        PrinterAxis::Y => "Y",
        PrinterAxis::Z => "Z",
    }
}

fn format_gcode_number(value: f64) -> String {
    value.to_string()
}

#[cfg(test)]
pub(crate) mod pause {
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
        time::Duration,
    };

    use tokio::sync::oneshot;

    const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) enum Phase {
        BeforeFinalLease,
        AfterFinalReadBeforePublish,
    }

    struct PausePoint {
        reached: oneshot::Sender<()>,
        resume: oneshot::Receiver<()>,
    }

    pub(crate) struct DispatchPause {
        reached: oneshot::Receiver<()>,
        resume: Option<oneshot::Sender<()>>,
    }

    pub(crate) fn install(serial: &str, phase: Phase) -> DispatchPause {
        let (reached_sender, reached_receiver) = oneshot::channel();
        let (resume_sender, resume_receiver) = oneshot::channel();
        let previous = pauses().lock().unwrap().insert(
            (serial.to_owned(), phase),
            PausePoint {
                reached: reached_sender,
                resume: resume_receiver,
            },
        );
        assert!(
            previous.is_none(),
            "feature dispatch pause already installed"
        );
        DispatchPause {
            reached: reached_receiver,
            resume: Some(resume_sender),
        }
    }

    impl DispatchPause {
        pub(crate) async fn wait_until_reached(&mut self) {
            tokio::time::timeout(WAIT_TIMEOUT, &mut self.reached)
                .await
                .expect("timed out waiting for feature dispatch pause")
                .expect("feature dispatch pause was dropped before being reached");
        }

        pub(crate) fn resume(mut self) {
            let _ = self.resume.take().unwrap().send(());
        }
    }

    pub(super) async fn wait(serial: &str, phase: Phase) {
        let pause = pauses().lock().unwrap().remove(&(serial.to_owned(), phase));
        if let Some(pause) = pause {
            let _ = pause.reached.send(());
            let _ = pause.resume.await;
        }
    }

    fn pauses() -> &'static Mutex<HashMap<(String, Phase), PausePoint>> {
        static PAUSES: OnceLock<Mutex<HashMap<(String, Phase), PausePoint>>> = OnceLock::new();
        PAUSES.get_or_init(|| Mutex::new(HashMap::new()))
    }
}
