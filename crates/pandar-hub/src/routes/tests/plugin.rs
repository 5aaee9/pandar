use super::*;
use requests::{
    plugin_login_ticket_body, plugin_ticket_exchange_body, redacted_audit_metadata_fixture,
    safe_audit_metadata_fixture,
};
use serde::{Deserialize, Serialize, de::IgnoredAny};

mod audit;
mod authorization;
mod firmware;
mod firmware_batch;
mod live_status;
mod login_tickets;
mod operations;
mod printers;
mod printing;
mod requests;
mod session_revocation;
mod studio_print;

#[derive(Debug, Deserialize)]
struct LoginTicketResponse {
    ticket: String,
    expires_at: String,
    redirect_url: String,
}

#[derive(Debug, Deserialize)]
struct ExchangeLoginTicketResponse {
    token: String,
    expires_at: String,
    profile: PluginProfileResponse,
}

#[derive(Debug, Deserialize)]
struct PluginProfileResponse {
    tenant_id: String,
    tenant_name: String,
}

#[derive(Debug, Deserialize)]
struct PluginPrinterListResponse {
    message: String,
    devices: Vec<PluginPrinterResponse>,
    #[serde(default)]
    printers: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct PluginPrinterResponse {
    dev_id: String,
    fun: String,
    dev_name: String,
    name: String,
    dev_model_name: Option<String>,
    model: Option<String>,
    dev_online: bool,
    online: bool,
    task_status: String,
    state: String,
    gcode_state: Option<String>,
    mc_percent: Option<u8>,
    mc_remaining_time: Option<u32>,
    layer_num: Option<u32>,
    total_layer_num: Option<u32>,
    task_id: Option<String>,
    subtask_id: Option<String>,
    gcode_file: Option<String>,
    subtask_name: Option<String>,
    hms: Vec<PluginPrinterHmsResponse>,
    pandar_printer_id: String,
    nozzle_temperatures: Vec<PluginNozzleTemperatureResponse>,
    active_nozzle: Option<String>,
    bed_temperature_celsius: Option<String>,
    bed_target_temperature_celsius: Option<String>,
    chamber_temperature_celsius: Option<String>,
    chamber_light_on: Option<bool>,
    materials: Option<PluginMaterialsResponse>,
    firmware: Option<pandar_core::PrinterFirmwareState>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct PluginPrinterHmsResponse {
    attr: u32,
    code: u32,
}

#[derive(Debug, Deserialize)]
struct PluginNozzleTemperatureResponse {
    current_celsius: Option<String>,
    target_celsius: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PluginMaterialsResponse {
    ams_units: Vec<PluginAmsUnitResponse>,
    external_spools: Vec<PluginExternalSpoolResponse>,
    active_tray: PluginActiveTrayResponse,
    cfg: String,
    aux: String,
    stat: String,
}

#[derive(Debug, Deserialize)]
struct PluginAmsUnitResponse {
    unit_id: String,
    info: String,
}

#[derive(Debug, Deserialize)]
struct PluginExternalSpoolResponse {
    external_id: String,
}

#[derive(Debug, Deserialize)]
struct PluginActiveTrayResponse {
    global_tray_id: u32,
}

#[derive(Debug, Deserialize)]
struct PluginPrintResponse {
    task_id: i32,
    studio_submission_id: i32,
    status: String,
}

#[derive(Debug, Deserialize)]
struct PluginJobListResponse {
    total: u64,
    hits: Vec<PluginJobResponse>,
}

#[derive(Debug, Deserialize)]
struct PluginJobResponse {
    id: i32,
    title: String,
}

#[derive(Debug, Deserialize)]
struct PluginTokenAuditMetadata {
    tenant_token_id: String,
    tenant_token_scopes: Vec<String>,
    token: Option<String>,
    ticket: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuditEventListResponse<T> {
    audit_events: Vec<AuditEventResponse<T>>,
}

#[derive(Debug, Deserialize)]
struct AuditEventResponse<T> {
    action: String,
    metadata: T,
}

#[derive(Debug, Deserialize)]
struct RedactedAuditMetadata {
    safe: String,
    nested: RedactedNestedAuditMetadata,
    subject: Option<String>,
    plaintext_token: Option<String>,
    ticket: Option<String>,
    plaintext_ticket: Option<String>,
    headers: RedactedHeaders,
    artifact_storage_path: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RedactedNestedAuditMetadata {
    ok: bool,
}

#[derive(Debug, Deserialize)]
struct RedactedHeaders {
    #[serde(rename = "Authorization")]
    authorization: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyAuditMetadata {}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Serialize)]
struct PluginMaterialPatchFixture {
    #[serde(rename = "type")]
    kind: &'static str,
    observed_at: &'static str,
    cfg: &'static str,
    aux: &'static str,
    stat: &'static str,
    ams_units: [PluginMaterialPatchAmsUnit; 1],
    external_spools: [PluginMaterialPatchExternalSpool; 1],
    active_tray: PluginMaterialPatchActiveTray,
}

#[derive(Debug, Serialize)]
struct PluginMaterialPatchAmsUnit {
    unit_id: &'static str,
    info: &'static str,
    humidity: u8,
    humidity_level: u8,
    temperature_celsius: f64,
    toolhead: &'static str,
    trays: [PluginMaterialPatchTray; 1],
}

#[derive(Debug, Serialize)]
struct PluginMaterialPatchTray {
    tray_id: &'static str,
    global_tray_id: u8,
    #[serde(rename = "type")]
    material_type: &'static str,
    filament_id: &'static str,
    color: &'static str,
    remaining_estimate: &'static str,
}

#[derive(Debug, Serialize)]
struct PluginMaterialPatchExternalSpool {
    external_id: &'static str,
    tray_id: &'static str,
    #[serde(rename = "type")]
    material_type: &'static str,
    filament_id: &'static str,
    color: &'static str,
    toolhead: &'static str,
}

#[derive(Debug, Serialize)]
struct PluginMaterialPatchActiveTray {
    kind: &'static str,
    ams_id: &'static str,
    tray_id: &'static str,
    global_tray_id: u8,
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> T {
    decode_json(value)
}

async fn insert_audit_fixture(
    state: &AppState,
    tenant_id: TenantId,
    action: &str,
    created_at: &str,
    metadata: Value,
) {
    insert_raw_audit_fixture(state, tenant_id, action, created_at, &metadata.to_string()).await;
}

async fn feature_advertisement_printer(
    state: &AppState,
    tenant_id: TenantId,
    agent_name: &str,
    serial: &str,
) -> pandar_core::AgentId {
    let agent = state.agents().create(tenant_id, agent_name).await.unwrap();
    state
        .printers()
        .upsert_snapshot(
            tenant_id,
            agent.id,
            crate::repositories::PrinterSnapshotUpsert {
                serial_number: serial.to_owned(),
                host: Some("192.0.2.10".to_owned()),
                access_code: Some("feature-access".to_owned()),
                name: serial.to_owned(),
                model: Some("X2D".to_owned()),
                status: Some("idle".to_owned()),
                observed_at: "2026-07-11T00:00:00Z".to_owned(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: Some("L".to_owned()),
                bed_temperature_celsius: Some("60".to_owned()),
                bed_target_temperature_celsius: Some("65".to_owned()),
                chamber_temperature_celsius: Some("32".to_owned()),
                chamber_target_temperature_celsius: None,
                chamber_light_on: Some(true),
                connection_authoritative: false,
                telemetry_authoritative: true,
            },
        )
        .await
        .unwrap();
    agent.id
}

async fn register_feature_session(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: pandar_core::AgentId,
    capable: bool,
) -> crate::sessions::SessionToken {
    let token = crate::sessions::SessionToken::new();
    claim_feature_session(state, tenant_id, agent_id, token).await;
    let capabilities = capable
        .then_some(crate::protocol::agent::v1::AgentCapability::RequiredDeviceFeatures)
        .into_iter()
        .collect();
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token,
            tenant_id,
            agent_id,
            name: "agent".to_owned(),
            version: "test".to_owned(),
            connected_at: "2026-07-11T00:00:00Z".to_owned(),
            last_heartbeat_at: "2026-07-11T00:00:00Z".to_owned(),
            wake_sender: tokio::sync::mpsc::channel(1).0,
            close_sender: tokio::sync::mpsc::channel(1).0,
            command_sender: tokio::sync::mpsc::channel(1).0,
            capabilities,
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;
    token
}

async fn claim_feature_session(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: pandar_core::AgentId,
    token: crate::sessions::SessionToken,
) {
    state
        .agents()
        .claim_online_session(
            tenant_id,
            agent_id,
            &token.persisted_id(),
            "test",
            "2026-07-11T00:00:00Z",
        )
        .await
        .unwrap();
}

async fn set_device_features(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: pandar_core::AgentId,
    token: crate::sessions::SessionToken,
    serial: &str,
    features: Option<pandar_core::BambuDeviceFeatures>,
) {
    assert_eq!(
        state
            .printers()
            .update_device_features_if_current(
                tenant_id,
                agent_id,
                &token.persisted_id(),
                serial,
                features,
            )
            .await
            .unwrap(),
        crate::repositories::DeviceFeatureUpdateOutcome::Updated
    );
}

async fn insert_raw_audit_fixture(
    state: &AppState,
    tenant_id: TenantId,
    action: &str,
    created_at: &str,
    metadata_json: &str,
) {
    sqlx::query(
        "INSERT INTO audit_events (id, tenant_id, actor_type, user_id, action, target_type, target_id, metadata_json, created_at)
         VALUES (?1, ?2, 'user', NULL, ?3, 'fixture', NULL, ?4, ?5)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(tenant_id.to_string())
    .bind(action)
    .bind(metadata_json)
    .bind(created_at)
    .execute(sqlite_pool(state))
    .await
    .unwrap();
}

fn sqlite_pool(state: &AppState) -> &sqlx::SqlitePool {
    let crate::db::Database::Sqlite(pool) = state.database() else {
        panic!("expected SQLite database");
    };
    pool
}
