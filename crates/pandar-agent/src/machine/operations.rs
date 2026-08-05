use anyhow::Context;
use pandar_core::{BambuDeviceFeature, H2cAutoNozzleMappingRequest};

mod axis;
mod light;
mod mqtt_command;
mod report;

use super::{
    BambuPrinterEndpoint, DeviceFeatureLease, PrinterOperationDispatchResult,
    mqtt::{
        BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics, BambuMqttTransport, PrintErrorAction,
        PublishedMqttCommand,
    },
};

pub(crate) use axis::operate_printer_with_feature_selection;
#[cfg(test)]
pub(crate) use axis::pause as device_feature_dispatch_pause;

#[derive(Debug, Clone, PartialEq)]
pub enum PrinterOperation {
    Pause,
    Resume,
    Stop,
    HandlePrintError {
        error_action: PrintErrorAction,
        print_error: u32,
        printer_job_id: String,
        sequence_id: u64,
    },
    ToggleLight,
    SetChamberLight(bool),
    SetPrintSpeed(u8),
    SelectExtruder(u32),
    Home {
        axes: Vec<PrinterAxis>,
        required_feature: Option<BambuDeviceFeature>,
    },
    MoveAxes {
        x_mm: Option<f64>,
        y_mm: Option<f64>,
        z_mm: Option<f64>,
        feedrate_mm_per_min: Option<f64>,
        required_feature: Option<BambuDeviceFeature>,
    },
    SetHotendTemperature {
        temperature_celsius: u16,
        wait: bool,
        extruder_id: Option<u32>,
    },
    SetBedTemperature {
        temperature_celsius: u16,
        wait: bool,
    },
    SetChamberTemperature {
        temperature_celsius: u16,
        wait: bool,
    },
    AmsRereadRfid {
        ams_id: u32,
        slot_id: u32,
    },
    AmsLoadFilament {
        ams_id: u32,
        slot_id: u32,
        global_tray_id: Option<u32>,
        external_id: Option<String>,
        extruder_id: Option<u32>,
    },
    AmsUnloadFilament {
        ams_id: u32,
        slot_id: u32,
        global_tray_id: Option<u32>,
        external_id: Option<String>,
        extruder_id: Option<u32>,
    },
    AmsStartDrying {
        ams_id: u32,
        temperature_celsius: u16,
        duration_hours: u16,
        filament: String,
        rotate_tray: bool,
    },
    AmsStopDrying {
        ams_id: u32,
    },
    GcodeLine {
        param: String,
    },
    GetAutoNozzleMapping(H2cAutoNozzleMappingRequest),
    NozzleHolderCtrl {
        action: u32,
    },
    NozzleInfoConfirm {
        id: u32,
    },
    HolderNozzleRefresh {
        id: u32,
    },
}

impl PrinterOperation {
    pub(crate) fn required_feature(&self) -> Option<BambuDeviceFeature> {
        match self {
            Self::Home {
                required_feature, ..
            }
            | Self::MoveAxes {
                required_feature, ..
            } => *required_feature,
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrinterAxis {
    X,
    Y,
    Z,
}

pub(super) async fn dispatch_printer_operation<T>(
    endpoint: &BambuPrinterEndpoint,
    mqtt: &T,
    operation: PrinterOperation,
    feature_lease: Option<DeviceFeatureLease>,
) -> anyhow::Result<PrinterOperationDispatchResult>
where
    T: BambuMqttTransport + Send + Sync,
{
    let expected_auto_mapping = match &operation {
        PrinterOperation::GetAutoNozzleMapping(request) => Some(request.clone()),
        _ => None,
    };
    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    mqtt.subscribe(&topics.report)
        .await
        .with_context(|| format!("subscribe to report topic {}", topics.report))?;
    let observed_features = feature_lease.as_ref().and_then(DeviceFeatureLease::get);
    let commands = match operation {
        PrinterOperation::ToggleLight => light::chamber_light_commands(mqtt, &topics, None).await?,
        PrinterOperation::SetChamberLight(on) => {
            light::chamber_light_commands(mqtt, &topics, Some(on)).await?
        }
        operation => vec![
            mqtt_command::mqtt_command_for_printer_operation_with_features(
                operation,
                observed_features,
            )
            .with_context(|| format!("select printer operation payload for {}", endpoint.serial))?,
        ],
    };
    let command_payloads = commands
        .iter()
        .map(BambuMqttCommand::command_payload)
        .collect::<Vec<_>>();
    let sequence_ids = command_payloads
        .iter()
        .filter_map(|payload| payload.sequence_id.clone())
        .collect::<Vec<_>>();
    for command_payload in command_payloads {
        mqtt.publish(PublishedMqttCommand {
            topic: topics.request.clone(),
            payload: command_payload.payload,
            qos: BAMBU_MQTT_QOS,
        })
        .await
        .with_context(|| format!("publish printer operation to {}", endpoint.serial))?;
    }
    drop(feature_lease);

    if sequence_ids.is_empty() {
        return Ok(PrinterOperationDispatchResult::dispatched());
    }

    match matching_sequence_report(
        mqtt,
        &sequence_ids,
        expected_auto_mapping
            .as_ref()
            .map(|_| "get_auto_nozzle_mapping"),
    )
    .await
    {
        Ok((sequence_id, report)) => {
            if let Some(request) = &expected_auto_mapping {
                let response = report
                    .auto_nozzle_mapping_response()
                    .context("decode H2C auto nozzle mapping response")?;
                if !response.is_valid_for(request) {
                    anyhow::bail!("printer returned an invalid H2C auto nozzle mapping response");
                }
            }
            let error = report.error();
            let mqtt_summary = report.summary();
            Ok(PrinterOperationDispatchResult {
                sequence_id: Some(sequence_id),
                error,
                mqtt_report: Some(report.into_payload()),
                mqtt_summary,
            })
        }
        Err(err) if expected_auto_mapping.is_some() => {
            Err(err).context("H2C auto nozzle mapping response unavailable")
        }
        Err(err) => {
            let sequence_id = sequence_ids
                .last()
                .expect("sequence ids are not empty")
                .clone();
            tracing::warn!(
                serial = %endpoint.serial,
                sequence_id = %sequence_id,
                error = %format!("{err:#}"),
                "printer operation result report unavailable"
            );
            Ok(PrinterOperationDispatchResult {
                sequence_id: Some(sequence_id),
                mqtt_report: None,
                mqtt_summary: None,
                error: None,
            })
        }
    }
}

pub(in crate::machine) use mqtt_command::mqtt_command_for_printer_operation;

async fn matching_sequence_report<T>(
    mqtt: &T,
    sequence_ids: &[String],
    expected_command: Option<&str>,
) -> anyhow::Result<(String, report::OperationReport)>
where
    T: BambuMqttTransport + Send + Sync,
{
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let mut failures = Vec::new();
        loop {
            let report = mqtt
                .next_report(std::time::Duration::from_secs(5))
                .await
                .context("wait for printer operation MQTT result")?;
            let Some(report) = report::OperationReport::from_payload(&report) else {
                continue;
            };
            let Some(sequence_id) = report.sequence_id() else {
                continue;
            };
            if !sequence_ids.contains(&sequence_id)
                || expected_command.is_some_and(|command| report.command() != Some(command))
            {
                continue;
            }
            if report.error().is_none() {
                return Ok((sequence_id, report));
            }
            failures.push((sequence_id, report));
            if failures.len() == sequence_ids.len() {
                return Ok(failures.remove(0));
            }
        }
    })
    .await
    .context("wait for matching printer operation MQTT result")?
}
