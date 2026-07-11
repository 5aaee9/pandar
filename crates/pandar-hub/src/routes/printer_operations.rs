mod live;
pub(crate) mod plate_mismatch;
mod request_field;
mod web_recovery;

pub(super) use web_recovery::dispatch_tenant_printer_operation;

use pandar_core::{CommandRecord, TenantId};
use request_field::RequestField;
use serde::Deserialize;

use crate::{
    AppState,
    repositories::{
        AuditActor, PrintErrorAction, PrinterAxis, PrinterAxisMovement, PrinterOperationKind,
        RepositoryError,
    },
    routes::ApiError,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PrinterOperationRequest {
    action: String,
    #[serde(default)]
    speed_mode: RequestField<u8>,
    #[serde(default)]
    axes: RequestField<Vec<PrinterAxis>>,
    #[serde(default)]
    movements: RequestField<Vec<PrinterAxisMovement>>,
    #[serde(default)]
    feedrate_mm_per_min: RequestField<u32>,
    #[serde(default)]
    temperature_celsius: RequestField<u16>,
    #[serde(default)]
    wait: RequestField<bool>,
    #[serde(default)]
    ams_id: RequestField<u32>,
    #[serde(default)]
    slot_id: RequestField<u32>,
    #[serde(default)]
    global_tray_id: RequestField<u32>,
    #[serde(default)]
    external_id: RequestField<String>,
    #[serde(default)]
    extruder_id: RequestField<u32>,
    #[serde(default)]
    light_on: RequestField<bool>,
    #[serde(default)]
    error_action: RequestField<PrintErrorAction>,
    #[serde(default)]
    print_error: RequestField<u32>,
    #[serde(default)]
    printer_job_id: RequestField<String>,
    #[serde(default)]
    sequence_id: RequestField<u64>,
    #[serde(default)]
    error_generation: RequestField<u64>,
}

pub(super) enum PluginPrinterOperation {
    Queued(PrinterOperationKind),
    Live(PrinterOperationKind),
}

pub(super) enum TenantPrinterOperation {
    Queued(PrinterOperationKind),
    HandlePrintError {
        error_action: PrintErrorAction,
        error_generation: u64,
    },
}

impl PrinterOperationRequest {
    pub(super) fn into_tenant_operation(self) -> Result<TenantPrinterOperation, ApiError> {
        if self.action == "handle_print_error" {
            if !self.no_operation_fields() || !self.no_plugin_transport_fields() {
                return Err(invalid_printer_control());
            }
            let (Some(error_action), Some(error_generation)) = (
                self.error_action.into_option(),
                self.error_generation.into_option(),
            ) else {
                return Err(invalid_printer_control());
            };
            return Ok(TenantPrinterOperation::HandlePrintError {
                error_action,
                error_generation,
            });
        }
        if !self.no_native_fields() {
            return Err(invalid_printer_control());
        }

        let operation: Result<PrinterOperationKind, ApiError> = match self.action.as_str() {
            "pause" if self.no_operation_fields() => Ok(PrinterOperationKind::Pause),
            "resume" if self.no_operation_fields() => Ok(PrinterOperationKind::Resume),
            "stop" if self.no_operation_fields() => Ok(PrinterOperationKind::Stop),
            "toggle_light" if self.no_operation_fields() => Ok(PrinterOperationKind::ToggleLight),
            "set_chamber_light"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.no_material_fields()
                    && self.extruder_id.is_missing()
                    && self.light_on.is_some() =>
            {
                Ok(PrinterOperationKind::SetChamberLight {
                    on: self.light_on.expect("checked above"),
                })
            }
            "set_print_speed"
                if self.speed_mode.is_some()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.no_ams_fields() =>
            {
                Ok(PrinterOperationKind::SetPrintSpeed {
                    speed_mode: self.speed_mode.expect("checked above"),
                })
            }
            "select_extruder"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.ams_id.is_missing()
                    && self.slot_id.is_missing()
                    && self.global_tray_id.is_missing()
                    && self.external_id.is_missing()
                    && self.extruder_id.is_some() =>
            {
                Ok(PrinterOperationKind::SelectExtruder {
                    extruder_id: self.extruder_id.expect("checked above"),
                })
            }
            "home"
                if self.speed_mode.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.no_ams_fields() =>
            {
                Ok(PrinterOperationKind::Home {
                    axes: self.axes.unwrap_or_default(),
                })
            }
            "move_axes"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.no_ams_fields() =>
            {
                Ok(PrinterOperationKind::MoveAxes {
                    movements: self.movements.unwrap_or_default(),
                    feedrate_mm_per_min: self.feedrate_mm_per_min.into_option(),
                })
            }
            "set_hotend_temperature"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.no_material_fields()
                    && self.temperature_celsius.is_some() =>
            {
                Ok(PrinterOperationKind::SetHotendTemperature {
                    temperature_celsius: self.temperature_celsius.expect("checked above"),
                    wait: self.wait.unwrap_or(false),
                    extruder_id: self.extruder_id.into_option(),
                })
            }
            "set_bed_temperature"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.no_ams_fields()
                    && self.temperature_celsius.is_some() =>
            {
                Ok(PrinterOperationKind::SetBedTemperature {
                    temperature_celsius: self.temperature_celsius.expect("checked above"),
                    wait: self.wait.unwrap_or(false),
                })
            }
            "set_chamber_temperature"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.no_ams_fields()
                    && self.temperature_celsius.is_some() =>
            {
                Ok(PrinterOperationKind::SetChamberTemperature {
                    temperature_celsius: self.temperature_celsius.expect("checked above"),
                    wait: self.wait.unwrap_or(false),
                })
            }
            "ams_reread_rfid"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.ams_id.is_some()
                    && self.slot_id.is_some()
                    && self.global_tray_id.is_missing()
                    && self.external_id.is_missing()
                    && self.extruder_id.is_missing() =>
            {
                Ok(PrinterOperationKind::AmsRereadRfid {
                    ams_id: self.ams_id.expect("checked above"),
                    slot_id: self.slot_id.expect("checked above"),
                })
            }
            "ams_load_filament"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.ams_id.is_some()
                    && self.slot_id.is_some() =>
            {
                Ok(PrinterOperationKind::AmsLoadFilament {
                    ams_id: self.ams_id.expect("checked above"),
                    slot_id: self.slot_id.expect("checked above"),
                    global_tray_id: self.global_tray_id.into_option(),
                    external_id: self.external_id.into_option(),
                    extruder_id: self.extruder_id.into_option(),
                })
            }
            "ams_unload_filament"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.ams_id.is_some()
                    && self.slot_id.is_some() =>
            {
                Ok(PrinterOperationKind::AmsUnloadFilament {
                    ams_id: self.ams_id.expect("checked above"),
                    slot_id: self.slot_id.expect("checked above"),
                    global_tray_id: self.global_tray_id.into_option(),
                    external_id: self.external_id.into_option(),
                    extruder_id: self.extruder_id.into_option(),
                })
            }
            _ => return Err(invalid_printer_control()),
        };
        let operation = operation?;
        Ok(TenantPrinterOperation::Queued(operation))
    }

    pub(super) fn into_plugin_operation(self) -> Result<PluginPrinterOperation, ApiError> {
        if self.action != "handle_print_error" {
            return self
                .into_tenant_operation()
                .and_then(|operation| match operation {
                    TenantPrinterOperation::Queued(operation) => {
                        Ok(PluginPrinterOperation::Queued(operation))
                    }
                    TenantPrinterOperation::HandlePrintError { .. } => {
                        Err(invalid_printer_control())
                    }
                });
        }
        if !self.no_operation_fields() || !self.error_generation.is_missing() {
            return Err(invalid_printer_control());
        }
        let (Some(error_action), Some(print_error), Some(printer_job_id), Some(sequence_id)) = (
            self.error_action.into_option(),
            self.print_error.into_option(),
            self.printer_job_id.into_option(),
            self.sequence_id.into_option(),
        ) else {
            return Err(invalid_printer_control());
        };
        if sequence_id == 0 {
            return Err(invalid_printer_control());
        }
        let operation = PrinterOperationKind::HandlePrintError {
            error_action,
            print_error,
            printer_job_id,
            sequence_id,
        };
        if !(1..=i32::MAX as u32).contains(&print_error) {
            return Err(invalid_printer_control());
        }
        Ok(PluginPrinterOperation::Live(operation))
    }

    fn no_operation_fields(&self) -> bool {
        self.speed_mode.is_missing()
            && self.axes.is_missing()
            && self.movements.is_missing()
            && self.feedrate_mm_per_min.is_missing()
            && self.temperature_celsius.is_missing()
            && self.wait.is_missing()
            && self.no_ams_fields()
            && self.light_on.is_missing()
    }

    fn no_ams_fields(&self) -> bool {
        self.no_material_fields() && self.extruder_id.is_missing() && self.light_on.is_missing()
    }

    fn no_material_fields(&self) -> bool {
        self.ams_id.is_missing()
            && self.slot_id.is_missing()
            && self.global_tray_id.is_missing()
            && self.external_id.is_missing()
    }

    fn no_native_fields(&self) -> bool {
        self.error_action.is_missing()
            && self.print_error.is_missing()
            && self.printer_job_id.is_missing()
            && self.sequence_id.is_missing()
            && self.error_generation.is_missing()
    }

    fn no_plugin_transport_fields(&self) -> bool {
        self.print_error.is_missing()
            && self.printer_job_id.is_missing()
            && self.sequence_id.is_missing()
    }
}

pub(super) async fn dispatch_plugin_printer_operation(
    state: &AppState,
    tenant_id: TenantId,
    printer_id: &str,
    request: PrinterOperationRequest,
    actor: AuditActor,
) -> Result<CommandRecord, ApiError> {
    match request.into_plugin_operation()? {
        PluginPrinterOperation::Queued(operation) => {
            let command = state
                .commands()
                .enqueue_printer_operation_with_audit(tenant_id, printer_id, operation, actor)
                .await
                .map_err(plugin_operation_error)?;
            state.wake_agent(command.tenant_id, command.agent_id).await;
            Ok(command)
        }
        PluginPrinterOperation::Live(operation) => {
            live::dispatch(state, tenant_id, printer_id, operation, actor).await
        }
    }
}

fn plugin_operation_error(error: RepositoryError) -> ApiError {
    match error {
        RepositoryError::PrinterControlUnavailable => live::printer_operation_unavailable(),
        other => other.into(),
    }
}

fn invalid_printer_control() -> ApiError {
    ApiError::bad_request("invalid_printer_control")
}
