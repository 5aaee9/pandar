use pandar_core::{
    PrinterCoolingFan, PrinterCoolingFanKind, PrinterCoolingMode, PrinterCoolingSystem,
    PrinterNozzleTemperature,
};

use super::*;

#[derive(Clone, Copy, Debug)]
enum TelemetryPatch {
    NozzleTemperatures,
    ActiveNozzle,
    BedCurrent,
    BedTarget,
    ChamberCurrent,
    ChamberTarget,
    ChamberLight,
    CoolingSystem,
}

const PATCHES: [TelemetryPatch; 8] = [
    TelemetryPatch::NozzleTemperatures,
    TelemetryPatch::ActiveNozzle,
    TelemetryPatch::BedCurrent,
    TelemetryPatch::BedTarget,
    TelemetryPatch::ChamberCurrent,
    TelemetryPatch::ChamberTarget,
    TelemetryPatch::ChamberLight,
    TelemetryPatch::CoolingSystem,
];

#[derive(Debug, PartialEq, Eq)]
struct StoredTelemetry {
    nozzle_temperatures: Vec<PrinterNozzleTemperature>,
    active_nozzle: Option<String>,
    bed_temperature_celsius: Option<String>,
    bed_target_temperature_celsius: Option<String>,
    chamber_temperature_celsius: Option<String>,
    chamber_target_temperature_celsius: Option<String>,
    chamber_light_on: Option<bool>,
    cooling_system: Option<PrinterCoolingSystem>,
}

impl StoredTelemetry {
    fn baseline() -> Self {
        Self {
            nozzle_temperatures: vec![nozzle("L", "41", "220")],
            active_nozzle: Some("L".to_owned()),
            bed_temperature_celsius: Some("60".to_owned()),
            bed_target_temperature_celsius: Some("65".to_owned()),
            chamber_temperature_celsius: Some("32".to_owned()),
            chamber_target_temperature_celsius: Some("45".to_owned()),
            chamber_light_on: Some(true),
            cooling_system: Some(cooling_system(PrinterCoolingMode::Heating, 70)),
        }
    }

    fn after(patch: TelemetryPatch) -> Self {
        let mut expected = Self::baseline();
        match patch {
            TelemetryPatch::NozzleTemperatures => {
                expected.nozzle_temperatures = vec![nozzle("R", "42", "230")];
            }
            TelemetryPatch::ActiveNozzle => expected.active_nozzle = Some("R".to_owned()),
            TelemetryPatch::BedCurrent => {
                expected.bed_temperature_celsius = Some("61".to_owned());
            }
            TelemetryPatch::BedTarget => {
                expected.bed_target_temperature_celsius = Some("66".to_owned());
            }
            TelemetryPatch::ChamberCurrent => {
                expected.chamber_temperature_celsius = Some("33".to_owned());
            }
            TelemetryPatch::ChamberTarget => {
                expected.chamber_target_temperature_celsius = Some("48".to_owned());
            }
            TelemetryPatch::ChamberLight => expected.chamber_light_on = Some(false),
            TelemetryPatch::CoolingSystem => {
                expected.cooling_system = Some(cooling_system(PrinterCoolingMode::Cooling, 100));
            }
        }
        expected
    }
}

pub(super) async fn exercise_partial_snapshot_presence(database: Database) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database);
    let tenant = tenants
        .create("snapshot-presence", "Snapshot Presence")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "snapshot-agent").await.unwrap();

    for patch in PATCHES {
        let serial = format!("SN-PRESENCE-{patch:?}");
        printers
            .upsert_snapshot(
                tenant.id,
                agent.id,
                full_snapshot(serial.clone(), "2026-07-20T00:00:00Z"),
            )
            .await
            .unwrap();

        let updated = printers
            .upsert_snapshot(
                tenant.id,
                agent.id,
                partial_snapshot(serial, patch, "2026-07-20T00:01:00Z"),
            )
            .await
            .unwrap();

        assert_eq!(
            telemetry(&updated),
            StoredTelemetry::after(patch),
            "{patch:?} must update only its explicitly present field"
        );
        assert_eq!(
            updated.model.as_deref(),
            Some("X2D"),
            "{patch:?} must not clear a model absent from the partial report"
        );
    }

    let serial = "SN-PRESENCE-AUTHORITATIVE".to_owned();
    printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            full_snapshot(serial.clone(), "2026-07-20T00:02:00Z"),
        )
        .await
        .unwrap();
    let mut authoritative =
        partial_snapshot(serial, TelemetryPatch::ChamberLight, "2026-07-20T00:03:00Z");
    authoritative.chamber_light_on = None;
    authoritative.telemetry_authoritative = true;
    let cleared = printers
        .upsert_snapshot(tenant.id, agent.id, authoritative)
        .await
        .unwrap();
    assert_eq!(
        telemetry(&cleared),
        StoredTelemetry {
            nozzle_temperatures: Vec::new(),
            active_nozzle: None,
            bed_temperature_celsius: None,
            bed_target_temperature_celsius: None,
            chamber_temperature_celsius: None,
            chamber_target_temperature_celsius: None,
            chamber_light_on: None,
            cooling_system: None,
        },
        "a matching full snapshot must clear telemetry absent from that full report"
    );
    assert_eq!(cleared.model.as_deref(), Some("X2D"));
}

pub(super) async fn exercise_mqtt_presence_session(database: Database) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database);
    let tenant = tenants
        .create("mqtt-presence-session", "MQTT Presence Session")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "mqtt-agent").await.unwrap();
    let session_one = "mqtt-presence-session-one";
    agents
        .claim_online_session(
            tenant.id,
            agent.id,
            session_one,
            "test",
            "2026-07-20T00:00:00Z",
        )
        .await
        .unwrap();

    let serial = "SN-MQTT-PRESENCE".to_owned();
    let first = printers
        .upsert_snapshot_with_device_features_if_current(
            tenant.id,
            agent.id,
            session_one,
            full_snapshot(serial.clone(), "2026-07-20T00:00:01Z"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(first.mqtt_presence_session_id.as_deref(), Some(session_one));

    let session_two = "mqtt-presence-session-two";
    agents
        .claim_online_session(
            tenant.id,
            agent.id,
            session_two,
            "test",
            "2026-07-20T00:01:00Z",
        )
        .await
        .unwrap();
    let partial = printers
        .upsert_snapshot_with_device_features_if_current(
            tenant.id,
            agent.id,
            session_two,
            partial_snapshot(
                serial.clone(),
                TelemetryPatch::ChamberCurrent,
                "2026-07-20T00:01:01Z",
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        partial.mqtt_presence_session_id.as_deref(),
        Some(session_one),
        "a partial report from a replacement session must not establish MQTT presence"
    );

    let second = printers
        .upsert_snapshot_with_device_features_if_current(
            tenant.id,
            agent.id,
            session_two,
            full_snapshot(serial, "2026-07-20T00:01:02Z"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        second.mqtt_presence_session_id.as_deref(),
        Some(session_two),
        "a matching full report must establish presence for the replacement session"
    );
}

#[tokio::test]
async fn sqlite_partial_snapshot_preserves_absent_telemetry_fields() {
    exercise_partial_snapshot_presence(sqlite_database().await).await;
}

#[tokio::test]
async fn sqlite_mqtt_presence_requires_an_authoritative_current_session_snapshot() {
    exercise_mqtt_presence_session(sqlite_database().await).await;
}

#[tokio::test]
async fn configured_connection_snapshot_does_not_clear_existing_telemetry() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database);
    let tenant = tenants
        .create("configured-presence", "Configured Presence")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "configured-agent").await.unwrap();
    let serial = "SN-CONFIGURED-PRESENCE".to_owned();
    printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            full_snapshot(serial.clone(), "2026-07-20T00:00:00Z"),
        )
        .await
        .unwrap();

    let mut configured = partial_snapshot(
        serial,
        TelemetryPatch::ChamberTarget,
        "2026-07-20T00:01:00Z",
    );
    configured.connection_authoritative = true;
    configured.status = None;
    configured.chamber_target_temperature_celsius = None;

    let updated = printers
        .upsert_snapshot(tenant.id, agent.id, configured)
        .await
        .unwrap();

    assert_eq!(telemetry(&updated), StoredTelemetry::baseline());
    assert_eq!(updated.status, "printing");
}

fn full_snapshot(serial_number: String, observed_at: &str) -> PrinterSnapshotUpsert {
    let telemetry = StoredTelemetry::baseline();
    PrinterSnapshotUpsert {
        serial_number,
        host: Some("192.0.2.10".to_owned()),
        access_code: Some("12345678".to_owned()),
        name: "Presence Printer".to_owned(),
        model: Some("X2D".to_owned()),
        status: Some("printing".to_owned()),
        observed_at: observed_at.to_owned(),
        nozzle_temperatures: telemetry.nozzle_temperatures,
        active_nozzle: telemetry.active_nozzle,
        bed_temperature_celsius: telemetry.bed_temperature_celsius,
        bed_target_temperature_celsius: telemetry.bed_target_temperature_celsius,
        chamber_temperature_celsius: telemetry.chamber_temperature_celsius,
        chamber_target_temperature_celsius: telemetry.chamber_target_temperature_celsius,
        chamber_light_on: telemetry.chamber_light_on,
        cooling_system: telemetry.cooling_system,
        nozzle_system: None,
        connection_authoritative: false,
        telemetry_authoritative: true,
    }
}

fn partial_snapshot(
    serial_number: String,
    patch: TelemetryPatch,
    observed_at: &str,
) -> PrinterSnapshotUpsert {
    let mut snapshot = PrinterSnapshotUpsert {
        serial_number,
        host: None,
        access_code: None,
        name: "Presence Printer".to_owned(),
        model: None,
        status: Some("printing".to_owned()),
        observed_at: observed_at.to_owned(),
        nozzle_temperatures: Vec::new(),
        active_nozzle: None,
        bed_temperature_celsius: None,
        bed_target_temperature_celsius: None,
        chamber_temperature_celsius: None,
        chamber_target_temperature_celsius: None,
        chamber_light_on: None,
        cooling_system: None,
        nozzle_system: None,
        connection_authoritative: false,
        telemetry_authoritative: false,
    };
    match patch {
        TelemetryPatch::NozzleTemperatures => {
            snapshot.nozzle_temperatures = vec![nozzle("R", "42", "230")];
        }
        TelemetryPatch::ActiveNozzle => snapshot.active_nozzle = Some("R".to_owned()),
        TelemetryPatch::BedCurrent => {
            snapshot.bed_temperature_celsius = Some("61".to_owned());
        }
        TelemetryPatch::BedTarget => {
            snapshot.bed_target_temperature_celsius = Some("66".to_owned());
        }
        TelemetryPatch::ChamberCurrent => {
            snapshot.chamber_temperature_celsius = Some("33".to_owned());
        }
        TelemetryPatch::ChamberTarget => {
            snapshot.chamber_target_temperature_celsius = Some("48".to_owned());
        }
        TelemetryPatch::ChamberLight => snapshot.chamber_light_on = Some(false),
        TelemetryPatch::CoolingSystem => {
            snapshot.cooling_system = Some(cooling_system(PrinterCoolingMode::Cooling, 100));
        }
    }
    snapshot
}

fn telemetry(printer: &pandar_core::Printer) -> StoredTelemetry {
    StoredTelemetry {
        nozzle_temperatures: printer.nozzle_temperatures.clone(),
        active_nozzle: printer.active_nozzle.clone(),
        bed_temperature_celsius: printer.bed_temperature_celsius.clone(),
        bed_target_temperature_celsius: printer.bed_target_temperature_celsius.clone(),
        chamber_temperature_celsius: printer.chamber_temperature_celsius.clone(),
        chamber_target_temperature_celsius: printer.chamber_target_temperature_celsius.clone(),
        chamber_light_on: printer.chamber_light_on,
        cooling_system: printer.cooling_system.clone(),
    }
}

fn cooling_system(mode: PrinterCoolingMode, speed_percent: u8) -> PrinterCoolingSystem {
    PrinterCoolingSystem {
        mode: Some(mode),
        fans: vec![PrinterCoolingFan {
            kind: PrinterCoolingFanKind::PartCooling,
            speed_percent,
        }],
    }
}

fn nozzle(label: &str, current: &str, target: &str) -> PrinterNozzleTemperature {
    PrinterNozzleTemperature {
        label: Some(label.to_owned()),
        current_celsius: Some(current.to_owned()),
        target_celsius: Some(target.to_owned()),
        diameter_mm: Some("0.4".to_owned()),
        nozzle_type: Some("hardened_steel".to_owned()),
        snow: None,
        hnow: None,
    }
}
