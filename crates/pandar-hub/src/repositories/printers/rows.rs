use anyhow::{Context, bail};
use pandar_core::{AgentId, BambuDeviceFeatures, Printer, PrinterParts, TenantId};

use crate::{
    entities::printers,
    printer_secrets::PrinterAccessCodeCipher,
    repositories::{RepositoryError, RepositoryResult},
};

pub(super) fn printer_from_model(
    model: printers::Model,
    access_code_cipher: &PrinterAccessCodeCipher,
) -> RepositoryResult<Printer> {
    (|| {
        let access_code = match (&model.access_code, &model.access_code_encrypted) {
            (Some(_), Some(_)) => {
                bail!(
                    "printer {} has both plaintext and encrypted access codes",
                    model.id
                )
            }
            (Some(_), None) => bail!("printer {} retains a plaintext access code", model.id),
            (None, Some(envelope)) => Some(
                access_code_cipher
                    .decrypt(&model.tenant_id, &model.serial_number, envelope)
                    .with_context(|| {
                        format!("decrypt persisted access code for printer {}", model.id)
                    })?,
            ),
            (None, None) => None,
        };

        Printer::from_parts(PrinterParts {
            id: model.id,
            tenant_id: TenantId::parse(&model.tenant_id).map_err(anyhow::Error::from)?,
            agent_id: AgentId::parse(&model.agent_id).map_err(anyhow::Error::from)?,
            serial_number: model.serial_number,
            host: model.host,
            access_code,
            name: model.name,
            model: model.model,
            status: model.status,
            last_seen_at: model
                .last_seen_at
                .context("failed to read printer last_seen_at")?,
            created_at: model.created_at,
            nozzle_temperatures: serde_json::from_str(&model.nozzle_temperatures_json)
                .context("failed to read printer nozzle temperatures")?,
            active_nozzle: model.active_nozzle,
            bed_temperature_celsius: model.bed_temperature_celsius,
            bed_target_temperature_celsius: model.bed_target_temperature_celsius,
            chamber_temperature_celsius: model.chamber_temperature_celsius,
            chamber_target_temperature_celsius: model.chamber_target_temperature_celsius,
            chamber_light_on: model.chamber_light_on,
            bambu_device_features: model
                .bambu_fun_bits
                .map(|value| BambuDeviceFeatures::from_hex(&value))
                .transpose()
                .context("failed to rehydrate printer Bambu device features")?,
            bambu_device_features_session_id: model.bambu_fun_session_id,
        })
        .map_err(anyhow::Error::from)
    })()
    .context("failed to rehydrate printer")
    .map_err(RepositoryError::from)
}
