use std::collections::HashMap;

use pandar_core::{PrinterNozzleTemperature, TenantId};
use serde::Serialize;

use crate::{AppState, printer_events::PrinterEventMaterials, routes::ApiError};

#[derive(Debug, Serialize)]
pub(crate) struct PluginPrinterListResponse {
    pub(super) devices: Vec<PluginPrinterResponse>,
}

#[derive(Debug, Serialize)]
pub(super) struct PluginPrinterResponse {
    dev_id: String,
    dev_name: String,
    name: String,
    dev_ip: Option<String>,
    dev_access_code: Option<String>,
    dev_model_name: Option<String>,
    model: Option<String>,
    dev_online: bool,
    online: bool,
    task_status: String,
    state: String,
    pandar_printer_id: String,
    nozzle_temperatures: Vec<PrinterNozzleTemperature>,
    active_nozzle: Option<String>,
    bed_temperature_celsius: Option<String>,
    bed_target_temperature_celsius: Option<String>,
    chamber_temperature_celsius: Option<String>,
    chamber_light_on: Option<bool>,
    materials: Option<PrinterEventMaterials>,
}

pub(super) async fn plugin_printer_devices(
    state: &AppState,
    tenant_id: TenantId,
) -> Result<Vec<PluginPrinterResponse>, ApiError> {
    let mut materials_by_printer_id = state
        .materials()
        .list_for_tenant(tenant_id)
        .await?
        .into_iter()
        .map(|snapshot| {
            (
                snapshot.printer_id.clone(),
                PrinterEventMaterials::from(snapshot),
            )
        })
        .collect::<HashMap<_, _>>();

    Ok(state
        .printers()
        .list_for_tenant(tenant_id)
        .await?
        .into_iter()
        .map(|printer| {
            let online = studio_online_from_status(&printer.status);
            let studio_model_name = printer.model.as_deref().map(studio_model_id);
            PluginPrinterResponse {
                dev_id: printer.serial_number.clone(),
                dev_name: printer.name.clone(),
                name: printer.name,
                dev_ip: printer.host,
                dev_access_code: printer.access_code,
                dev_model_name: studio_model_name,
                model: printer.model,
                dev_online: online,
                online,
                task_status: printer.status.clone(),
                state: printer.status,
                nozzle_temperatures: printer.nozzle_temperatures,
                active_nozzle: printer.active_nozzle,
                bed_temperature_celsius: printer.bed_temperature_celsius,
                bed_target_temperature_celsius: printer.bed_target_temperature_celsius,
                chamber_temperature_celsius: printer.chamber_temperature_celsius,
                chamber_light_on: printer.chamber_light_on,
                materials: materials_by_printer_id.remove(&printer.id),
                pandar_printer_id: printer.id,
            }
        })
        .collect())
}

fn studio_online_from_status(status: &str) -> bool {
    let normalized = status.trim().to_ascii_lowercase();
    !matches!(normalized.as_str(), "offline" | "unknown")
}

fn studio_model_id(model: &str) -> String {
    let compact = model
        .chars()
        .filter(|value| value.is_ascii_alphanumeric())
        .flat_map(char::to_uppercase)
        .collect::<String>();
    match compact.as_str() {
        "N6" | "X2D" | "BAMBULABX2D" => "N6".to_string(),
        "N7" | "P2S" | "BAMBULABP2S" => "N7".to_string(),
        _ => model.to_string(),
    }
}
