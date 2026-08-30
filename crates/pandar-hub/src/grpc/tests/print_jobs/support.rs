use super::*;

pub(super) async fn create_print_job(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    artifact_id: &str,
) -> crate::repositories::JobWithArtifact {
    create_print_job_with_mappings(state, tenant_id, agent_id, artifact_id, None, None).await
}

pub(super) async fn create_print_job_with_mappings(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    artifact_id: &str,
    ams_mapping_json: Option<String>,
    ams_mapping2_json: Option<String>,
) -> crate::repositories::JobWithArtifact {
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    state
        .jobs()
        .create_print_job(print_input(
            tenant_id,
            agent_id,
            &printer_id,
            artifact_id,
            ams_mapping_json,
            ams_mapping2_json,
        ))
        .await
        .unwrap()
}

pub(super) fn print_input(
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: &str,
    artifact_id: &str,
    ams_mapping_json: Option<String>,
    ams_mapping2_json: Option<String>,
) -> CreatePrintJob {
    CreatePrintJob {
        tenant_id,
        printer_id: printer_id.to_owned(),
        agent_id,
        artifact: crate::repositories::PrintArtifactInput {
            id: artifact_id.to_owned(),
            filename: "plate.3mf".to_owned(),
            content_type: "model/3mf".to_owned(),
            size_bytes: 42,
            storage_path: format!("{tenant_id}/{artifact_id}/plate.3mf"),
            metadata_json: None,
        },
        options: crate::repositories::PrintExecutionOptions {
            plate_id: 1,
            use_ams: true,
            auto_bed_leveling: pandar_core::PrintCalibrationMode::Auto,
            bed_leveling: true,
            flow_cali: false,
            auto_flow_cali: pandar_core::PrintCalibrationMode::On,
            auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
            timelapse: true,
            ams_mapping_json,
            ams_mapping2_json,
            ams_mapping_info_json: None,
        },
    }
}

pub(super) async fn corrupt_command_payload(state: &AppState, command_id: pandar_core::CommandId) {
    match state.database() {
        Database::Sqlite(pool) => {
            sqlx::query("UPDATE commands SET payload_json = ?2 WHERE id = ?1")
                .bind(command_id.to_string())
                .bind("{")
                .execute(pool)
                .await
                .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query("UPDATE commands SET payload_json = $2 WHERE id = $1")
                .bind(command_id.to_string())
                .bind("{")
                .execute(pool)
                .await
                .unwrap();
        }
    }
}

pub(super) async fn corrupt_command_mapping(state: &AppState, command_id: pandar_core::CommandId) {
    let payload = r#"{"job_id":"job","artifact_id":"artifact","printer_id":"printer","serial_number":"serial","filename":"plate.3mf","storage_path":"tenant/artifact/plate.3mf","artifact_download_path":"/api/v1/agents/agent/artifacts/artifact","size_bytes":42,"plate_id":1,"use_ams":true,"bed_leveling":true,"auto_bed_leveling":2,"flow_cali":false,"auto_flow_cali":1,"auto_offset_cali":0,"timelapse":true,"ams_mapping_json":"[{}]","ams_mapping2_json":null}"#;
    match state.database() {
        Database::Sqlite(pool) => {
            sqlx::query("UPDATE commands SET payload_json = ?2 WHERE id = ?1")
                .bind(command_id.to_string())
                .bind(payload)
                .execute(pool)
                .await
                .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query("UPDATE commands SET payload_json = $2 WHERE id = $1")
                .bind(command_id.to_string())
                .bind(payload)
                .execute(pool)
                .await
                .unwrap();
        }
    }
}
