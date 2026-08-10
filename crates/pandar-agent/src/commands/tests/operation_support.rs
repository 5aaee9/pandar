use super::*;

pub(super) fn pause_operation_command(command_id: String, serial_number: &str) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::Pause(PauseOperation {})),
    )
}

pub(super) fn resume_operation_command(command_id: String, serial_number: &str) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::Resume(
            crate::protocol::agent::v1::ResumeOperation {},
        )),
    )
}

pub(super) fn set_print_speed_operation_command(
    command_id: String,
    serial_number: &str,
    speed_mode: u32,
) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::SetPrintSpeed(
            SetPrintSpeedOperation { speed_mode },
        )),
    )
}

pub(super) fn set_fan_speed_operation_command(
    command_id: String,
    serial_number: &str,
    fan_index: u32,
    speed_percent: u32,
    airduct: bool,
) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::SetFanSpeed(
            SetFanSpeedOperation {
                fan_index,
                speed_percent,
                airduct,
            },
        )),
    )
}

pub(super) fn select_extruder_operation_command(
    command_id: String,
    serial_number: &str,
    extruder_id: u32,
) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::SelectExtruder(
            SelectExtruderOperation { extruder_id },
        )),
    )
}

pub(super) fn home_operation_command(
    command_id: String,
    serial_number: &str,
    axes: Vec<i32>,
) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::Home(HomeOperation { axes })),
    )
}

pub(super) fn move_axes_operation_command(command_id: String, serial_number: &str) -> HubCommand {
    move_axes_operation_command_with_movements(
        command_id,
        serial_number,
        vec![
            AxisMovement {
                axis: Axis::X as i32,
                delta_mm: 10.0,
            },
            AxisMovement {
                axis: Axis::Z as i32,
                delta_mm: -0.5,
            },
        ],
        3000,
    )
}

pub(super) fn move_axes_operation_command_with_movements(
    command_id: String,
    serial_number: &str,
    movements: Vec<AxisMovement>,
    feedrate_mm_per_min: u32,
) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::MoveAxes(MoveAxesOperation {
            movements,
            feedrate_mm_per_min,
        })),
    )
}

pub(super) fn hotend_operation_command(
    command_id: String,
    serial_number: &str,
    temperature_celsius: u32,
    wait: bool,
) -> HubCommand {
    hotend_operation_command_with_extruder(
        command_id,
        serial_number,
        temperature_celsius,
        wait,
        None,
    )
}

pub(super) fn hotend_operation_command_with_extruder(
    command_id: String,
    serial_number: &str,
    temperature_celsius: u32,
    wait: bool,
    extruder_id: Option<u32>,
) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::SetHotendTemperature(
            SetHotendTemperatureOperation {
                temperature_celsius,
                wait,
                extruder_id,
            },
        )),
    )
}

pub(super) fn bed_temperature_operation_command(
    command_id: String,
    serial_number: &str,
    temperature_celsius: u32,
    wait: bool,
) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::SetBedTemperature(
            SetBedTemperatureOperation {
                temperature_celsius,
                wait,
            },
        )),
    )
}

pub(super) fn chamber_temperature_operation_command(
    command_id: String,
    serial_number: &str,
    temperature_celsius: u32,
    wait: bool,
) -> HubCommand {
    printer_operation_command(
        command_id,
        serial_number,
        Some(printer_operation::Operation::SetChamberTemperature(
            SetChamberTemperatureOperation {
                temperature_celsius,
                wait,
            },
        )),
    )
}

pub(super) fn printer_operation_command(
    command_id: String,
    serial_number: &str,
    operation: Option<printer_operation::Operation>,
) -> HubCommand {
    printer_operation_command_with_required_features(
        command_id,
        serial_number,
        Vec::new(),
        operation,
    )
}

pub(super) fn printer_operation_command_with_required_features(
    command_id: String,
    serial_number: &str,
    required_device_features: Vec<i32>,
    operation: Option<printer_operation::Operation>,
) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::PrinterOperation(
            ProtoPrinterOperation {
                serial_number: serial_number.to_owned(),
                required_device_features,
                operation,
            },
        )),
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct OperationGateway {
    operations: Arc<Mutex<Vec<(String, MachinePrinterOperation)>>>,
    validate_error: Option<String>,
    dispatch_error: Option<String>,
    material_result: Option<Arc<Mutex<anyhow::Result<MaterialRefreshResult>>>>,
    access_code: Option<String>,
}

impl OperationGateway {
    pub(super) fn unknown_serial() -> Self {
        Self {
            validate_error: Some("no configured Bambu printer matches serial UNKNOWN".to_string()),
            ..Self::default()
        }
    }

    pub(super) fn publish_failure(access_code: &str) -> Self {
        Self {
            dispatch_error: Some(format!(
                "fake publish failure with access code {access_code}"
            )),
            access_code: Some(access_code.to_string()),
            ..Self::default()
        }
    }

    pub(super) fn with_materials(materials: MaterialRefreshResult) -> Self {
        Self {
            material_result: Some(Arc::new(Mutex::new(Ok(materials)))),
            ..Self::default()
        }
    }

    pub(super) async fn operations(&self) -> Vec<(String, MachinePrinterOperation)> {
        self.operations.lock().await.clone()
    }
}

#[async_trait]
impl BambuMachineGateway for OperationGateway {
    fn redact_error(&self, message: &str) -> String {
        match &self.access_code {
            Some(access_code) => message.replace(access_code, "[REDACTED_ACCESS_CODE]"),
            None => message.to_owned(),
        }
    }

    async fn discover_printers(
        &self,
        _timeout_seconds: u32,
    ) -> anyhow::Result<PrinterDiscoveryResult> {
        unreachable!("printer operation tests do not discover printers")
    }

    async fn diagnose_printer(
        &self,
        _serial_number: &str,
    ) -> anyhow::Result<PrinterDiagnosticResult> {
        unreachable!("printer operation tests do not diagnose printers")
    }

    async fn refresh_printers(&self) -> anyhow::Result<Vec<PrinterRefreshResult>> {
        unreachable!("printer operation tests do not refresh printers")
    }

    async fn refresh_printer_materials(
        &self,
        _serial_number: &str,
        _printer_id: Option<&str>,
    ) -> anyhow::Result<MaterialRefreshResult> {
        let Some(result) = &self.material_result else {
            unreachable!("printer operation tests do not refresh printer materials")
        };
        let mut result = result.lock().await;
        std::mem::replace(
            &mut *result,
            Err(anyhow::anyhow!("unexpected material refresh")),
        )
    }

    async fn validate_printer(&self, _serial_number: &str) -> anyhow::Result<()> {
        match &self.validate_error {
            Some(error) => Err(anyhow::anyhow!(error.clone())),
            None => Ok(()),
        }
    }

    async fn print_project_file(
        &self,
        _serial_number: &str,
        _command: &crate::protocol::agent::v1::PrintProjectFile,
        _artifact: Vec<u8>,
    ) -> anyhow::Result<PrintProjectDispatchResult> {
        unreachable!("printer operation tests do not dispatch print commands")
    }

    async fn operate_printer(
        &self,
        serial_number: &str,
        operation: MachinePrinterOperation,
    ) -> anyhow::Result<crate::machine::PrinterOperationDispatchResult> {
        self.operations
            .lock()
            .await
            .push((serial_number.to_string(), operation));
        match &self.dispatch_error {
            Some(error) => Err(anyhow::anyhow!(error.clone())),
            None => Ok(crate::machine::PrinterOperationDispatchResult::dispatched()),
        }
    }
}
