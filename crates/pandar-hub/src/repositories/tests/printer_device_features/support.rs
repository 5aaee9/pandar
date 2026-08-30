use super::*;

pub(in crate::repositories::tests) async fn claim_session(
    agents: &AgentRepository,
    tenant_id: TenantId,
    agent_id: pandar_core::AgentId,
    session_id: &str,
) {
    agents
        .claim_online_session(
            tenant_id,
            agent_id,
            session_id,
            "test",
            "2026-07-11T00:00:00Z",
        )
        .await
        .unwrap();
}

pub(in crate::repositories::tests) fn rich_snapshot(
    serial_number: &str,
    status: &str,
) -> PrinterSnapshotUpsert {
    PrinterSnapshotUpsert {
        serial_number: serial_number.to_owned(),
        host: Some("192.0.2.55".to_owned()),
        access_code: Some("feature-access".to_owned()),
        name: "Feature Printer".to_owned(),
        model: Some("X2D".to_owned()),
        status: Some(status.to_owned()),
        observed_at: "2026-07-11T00:01:00Z".to_owned(),
        nozzle_temperatures: vec![PrinterNozzleTemperature {
            label: Some("L".to_owned()),
            current_celsius: Some("41".to_owned()),
            target_celsius: Some("220".to_owned()),
            diameter_mm: Some("0.4".to_owned()),
            nozzle_type: Some("Hardened steel".to_owned()),
            snow: None,
            hnow: None,
        }],
        active_nozzle: Some("L".to_owned()),
        bed_temperature_celsius: Some("60".to_owned()),
        bed_target_temperature_celsius: Some("65".to_owned()),
        chamber_temperature_celsius: Some("32".to_owned()),
        chamber_target_temperature_celsius: None,
        chamber_light_on: Some(true),
        cooling_system: None,
        nozzle_system: None,
        connection_authoritative: false,
        telemetry_authoritative: true,
    }
}

pub(super) fn nozzle_system(id: i32) -> BambuNozzleSystem {
    BambuNozzleSystem {
        nozzle: BambuNozzleDevice {
            exist: Some(1 << id),
            state: Some(0),
            src_id: Some(id),
            tar_id: None,
            info: vec![BambuNozzleInfo {
                id,
                diameter: StudioFiniteF64::try_from(0.4).unwrap(),
                nozzle_type: "XS01".to_owned(),
                stat: Some(0),
                fila_id: None,
                wear: None,
                p_t: None,
                color_m: None,
            }],
        },
        holder: None,
    }
}

pub(super) async fn stored_printer(
    database: &Database,
    tenant_id: TenantId,
    serial: &str,
) -> printers::Model {
    printers::Entity::find()
        .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
        .filter(printers::Column::SerialNumber.eq(serial))
        .one(&database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap()
}
