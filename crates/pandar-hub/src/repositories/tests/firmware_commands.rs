use pandar_core::{CommandStatus, FirmwareControlMetadata, FirmwareTerminalOutcome};
use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

use super::*;
use crate::entities::commands as command_rows;
use crate::repositories::{
    AuditActor, FirmwareCommandOwner, FirmwareControlPayload, FirmwarePersistedPhase,
    FirmwarePersistedResult, FirmwareRefreshPayload, test_helpers::insert_printer_fixture,
};

#[tokio::test]
async fn firmware_command_repository_sqlite_is_url_free_and_typed() {
    let database = sqlite_database().await;
    verify_firmware_commands(database, "firmware-command-sqlite").await;
}

#[tokio::test]
async fn firmware_command_repository_postgres_matches_sqlite_when_configured() {
    let Some(database) = super::postgres::postgres_database().await else {
        eprintln!(
            "PANDAR_TEST_POSTGRES_URL is unset; real PostgreSQL firmware command verification skipped"
        );
        return;
    };
    verify_firmware_commands(database, "firmware-command-postgres").await;
}

#[tokio::test]
async fn firmware_command_cleanup_uses_authoritative_owner_liveness_on_sqlite() {
    let database = sqlite_database().await;
    verify_firmware_cleanup_owner_liveness(database, "firmware-cleanup-sqlite").await;
}

#[tokio::test]
async fn firmware_command_cleanup_uses_authoritative_owner_liveness_on_postgres_when_configured() {
    let Some(database) = super::postgres::postgres_database().await else {
        eprintln!(
            "PANDAR_TEST_POSTGRES_URL is unset; real PostgreSQL firmware cleanup verification skipped"
        );
        return;
    };
    verify_firmware_cleanup_owner_liveness(database, "firmware-cleanup-postgres").await;
}

async fn verify_firmware_cleanup_owner_liveness(database: Database, slug: &str) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let tenant = tenants.create(slug, "Firmware Cleanup").await.unwrap();
    let agent = agents.create(tenant.id, "firmware-agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let session_id = format!("{slug}-session");
    let owner = FirmwareCommandOwner {
        session_id: session_id.clone(),
        instance_id: uuid::Uuid::new_v4(),
    };
    let sweeper_instance_id = uuid::Uuid::new_v4();
    agents
        .claim_online_session(
            tenant.id,
            agent.id,
            &session_id,
            "firmware-test",
            "2026-07-12T00:06:00Z",
        )
        .await
        .unwrap();
    let command = commands
        .create_firmware_refresh_sent_with_audit(
            tenant.id,
            &printer_id,
            agent.id,
            owner.clone(),
            "cleanup-sequence".to_owned(),
            actor(),
        )
        .await
        .unwrap();
    let control = commands
        .create_firmware_control_sent_with_audit(
            tenant.id,
            &printer_id,
            agent.id,
            owner.clone(),
            FirmwareControlMetadata::UpgradeConfirm {
                sequence_id: "cleanup-control".to_owned(),
                src_id: 1,
            },
            actor(),
        )
        .await
        .unwrap();
    for command_id in [command.id, control.id] {
        set_command_updated_at(&database, command_id, "2026-07-12T00:00:00Z").await;
    }

    let failed = commands
        .fail_stale_unowned_live_commands(
            "2026-07-12T00:06:00Z",
            std::time::Duration::from_secs(300),
            std::time::Duration::from_secs(45),
            sweeper_instance_id,
            &[],
        )
        .await
        .unwrap();
    assert_eq!(failed, 0);
    for command_id in [command.id, control.id] {
        assert_eq!(
            commands
                .get_for_tenant(tenant.id, command_id)
                .await
                .unwrap()
                .unwrap()
                .status,
            CommandStatus::Sent
        );
    }

    agents
        .heartbeat_if_current(tenant.id, agent.id, &session_id, "2026-07-12T00:05:15Z")
        .await
        .unwrap();
    let failed = commands
        .fail_stale_unowned_live_commands(
            "2026-07-12T00:06:00Z",
            std::time::Duration::from_secs(300),
            std::time::Duration::from_secs(45),
            sweeper_instance_id,
            &[],
        )
        .await
        .unwrap();
    assert_eq!(failed, 2);
    for command_id in [command.id, control.id] {
        assert_eq!(
            commands
                .get_for_tenant(tenant.id, command_id)
                .await
                .unwrap()
                .unwrap()
                .status,
            CommandStatus::Failed
        );
    }

    let replaced = commands
        .create_firmware_refresh_sent_with_audit(
            tenant.id,
            &printer_id,
            agent.id,
            owner.clone(),
            "replaced-cleanup-sequence".to_owned(),
            actor(),
        )
        .await
        .unwrap();
    let replaced_control = commands
        .create_firmware_control_sent_with_audit(
            tenant.id,
            &printer_id,
            agent.id,
            owner,
            FirmwareControlMetadata::UpgradeConfirm {
                sequence_id: "replaced-cleanup-control".to_owned(),
                src_id: 2,
            },
            actor(),
        )
        .await
        .unwrap();
    for command_id in [replaced.id, replaced_control.id] {
        set_command_updated_at(&database, command_id, "2026-07-12T00:00:00Z").await;
    }
    let replacement_session_id = format!("{slug}-replacement-session");
    agents
        .claim_online_session(
            tenant.id,
            agent.id,
            &replacement_session_id,
            "firmware-test",
            "2026-07-12T00:06:00Z",
        )
        .await
        .unwrap();
    let failed = commands
        .fail_stale_unowned_live_commands(
            "2026-07-12T00:06:00Z",
            std::time::Duration::from_secs(300),
            std::time::Duration::from_secs(45),
            sweeper_instance_id,
            &[],
        )
        .await
        .unwrap();
    assert_eq!(failed, 2);
    for command_id in [replaced.id, replaced_control.id] {
        assert_eq!(
            commands
                .get_for_tenant(tenant.id, command_id)
                .await
                .unwrap()
                .unwrap()
                .status,
            CommandStatus::Failed
        );
    }
}

async fn verify_firmware_commands(database: Database, slug: &str) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let audit = AuditEventRepository::new(database.clone());
    let tenant = tenants.create(slug, "Firmware Commands").await.unwrap();
    let agent = agents.create(tenant.id, "firmware-agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let owner_session_id = format!("{slug}-session");
    let owner = FirmwareCommandOwner {
        session_id: owner_session_id.clone(),
        instance_id: uuid::Uuid::new_v4(),
    };
    let refresh = commands
        .create_firmware_refresh_sent_with_audit(
            tenant.id,
            &printer_id,
            agent.id,
            owner.clone(),
            "refresh-sequence".to_owned(),
            actor(),
        )
        .await
        .unwrap();
    assert_eq!(refresh.kind, "firmware_refresh");
    assert_eq!(refresh.status, CommandStatus::Sent);
    let refresh_payload: FirmwareRefreshPayload =
        serde_json::from_str(&refresh.payload_json).unwrap();
    assert_eq!(refresh_payload.owner_session_id, owner_session_id);
    assert_eq!(refresh_payload.owner_instance_id, owner.instance_id);
    let refresh = commands
        .mark_firmware_terminal(
            refresh.id,
            tenant.id,
            agent.id,
            CommandStatus::Succeeded,
            None,
            FirmwarePersistedResult {
                phase: FirmwarePersistedPhase::Refreshed,
                outcome: None,
                transient_status: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(refresh.status, CommandStatus::Succeeded);

    let control = commands
        .create_firmware_control_sent_with_audit(
            tenant.id,
            &printer_id,
            agent.id,
            owner.clone(),
            FirmwareControlMetadata::Start {
                sequence_id: "control-sequence".to_owned(),
                src_id: 1,
                module: "ota".to_owned(),
                version: "01.02.03".to_owned(),
            },
            actor(),
        )
        .await
        .unwrap();
    assert_eq!(control.kind, "firmware_control");
    assert_eq!(control.status, CommandStatus::Sent);
    let control_payload: FirmwareControlPayload =
        serde_json::from_str(&control.payload_json).unwrap();
    assert_eq!(control_payload.owner_session_id, owner_session_id);
    assert_eq!(control_payload.owner_instance_id, owner.instance_id);
    let generic = commands
        .mark_failed_with_result(
            control.id,
            tenant.id,
            agent.id,
            "generic firmware failure",
            Some("{\"url\":\"FIRMWARE-GENERIC-RESULT-SENTINEL\"}".to_owned()),
        )
        .await;
    assert!(matches!(
        generic,
        Err(RepositoryError::InvalidCommandTransition { .. })
    ));
    let unchanged = commands
        .get_for_tenant(tenant.id, control.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(unchanged.status, CommandStatus::Sent);
    assert!(
        !serde_json::to_string(&unchanged)
            .unwrap()
            .contains("FIRMWARE-GENERIC-RESULT-SENTINEL")
    );
    commands
        .mark_firmware_execute_sent(control.id, tenant.id, agent.id)
        .await
        .unwrap();
    let control = commands
        .mark_firmware_terminal(
            control.id,
            tenant.id,
            agent.id,
            CommandStatus::Failed,
            Some("outcome unknown".to_owned()),
            FirmwarePersistedResult {
                phase: FirmwarePersistedPhase::OutcomeUnknown,
                outcome: Some(FirmwareTerminalOutcome::PublishedWithoutAcknowledgement),
                transient_status: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(control.status, CommandStatus::Failed);
    let result: FirmwarePersistedResult =
        serde_json::from_str(control.result_json.as_deref().unwrap()).unwrap();
    assert_eq!(result.phase, FirmwarePersistedPhase::OutcomeUnknown);
    let readback = serde_json::to_string(&(
        refresh,
        control,
        audit.list_for_tenant(tenant.id).await.unwrap(),
    ))
    .unwrap();
    assert!(!readback.contains("url"));
    assert!(!readback.contains("signature"));
}

fn actor() -> AuditActor {
    AuditActor::plugin_token(None, "firmware-test-token", vec!["plugin"])
}

async fn set_command_updated_at(database: &Database, command_id: CommandId, updated_at: &str) {
    let result = command_rows::Entity::update_many()
        .set(command_rows::ActiveModel {
            updated_at: Set(updated_at.to_owned()),
            ..Default::default()
        })
        .filter(command_rows::Column::Id.eq(command_id.to_string()))
        .exec(&database.sea_orm_connection())
        .await
        .unwrap();
    assert_eq!(result.rows_affected, 1);
}
