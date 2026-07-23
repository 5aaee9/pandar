use tokio::sync::mpsc;

use super::*;
use crate::{
    grpc::commands::{
        SessionQueuedDispatch, dispatch_next_queued_for_session, required_feature_dispatch_pause,
    },
    protocol::agent::v1::hub_command,
    repositories::{AuditActor, CreatePrintJob},
};

#[tokio::test]
async fn cancellation_winning_dispatch_cas_skips_command_and_keeps_session_usable() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture(
        state.database(),
        tenant_id,
        agent_id,
    )
    .await
    .unwrap();
    let created = state
        .jobs()
        .create_studio_print_job_with_audit(
            create_input(tenant_id, agent_id, &printer_id),
            crate::test_support::studio_metadata_for_tests(),
            actor(),
        )
        .await
        .unwrap();
    let token = crate::grpc::register_test_session(&state, tenant_id, agent_id).await;
    let (sender, mut receiver) = mpsc::channel(2);
    let mut pause = required_feature_dispatch_pause::install(
        token,
        required_feature_dispatch_pause::Phase::AfterQueuedRowRead,
    );
    let dispatch_state = state.clone();
    let dispatch_sender = sender.clone();
    let dispatch = tokio::spawn(async move {
        dispatch_next_queued_for_session(
            &dispatch_state,
            tenant_id,
            agent_id,
            token,
            &dispatch_sender,
            CommandConversionOptions {
                require_artifact_download_path: false,
            },
        )
        .await
    });
    pause.wait_until_reached().await;
    state
        .jobs()
        .cancel_studio_print_with_audit(tenant_id, created.job.studio_submission_id, actor())
        .await
        .unwrap();
    pause.resume();

    assert_eq!(
        dispatch.await.unwrap().unwrap(),
        SessionQueuedDispatch::FailedAndContinue
    );
    assert!(receiver.try_recv().is_err());

    let next = state
        .commands()
        .enqueue_refresh_printers(tenant_id, agent_id)
        .await
        .unwrap();
    assert_eq!(
        dispatch_next_queued_for_session(
            &state,
            tenant_id,
            agent_id,
            token,
            &sender,
            CommandConversionOptions {
                require_artifact_download_path: false,
            },
        )
        .await
        .unwrap(),
        SessionQueuedDispatch::Sent
    );
    let sent = receiver.recv().await.unwrap().unwrap();
    assert_eq!(sent.command_id, next.id.to_string());
    assert!(matches!(
        sent.command,
        Some(hub_command::Command::RefreshPrinters(_))
    ));
}

fn create_input(tenant_id: TenantId, agent_id: AgentId, printer_id: &str) -> CreatePrintJob {
    CreatePrintJob {
        tenant_id,
        printer_id: printer_id.to_owned(),
        agent_id,
        artifact_id: "cancel-race-artifact".to_owned(),
        artifact_filename: "plate.3mf".to_owned(),
        artifact_content_type: "model/3mf".to_owned(),
        artifact_size_bytes: 42,
        artifact_storage_path: "cancel-race/plate.3mf".to_owned(),
        artifact_metadata_json: None,
        plate_id: 1,
        use_ams: false,
        auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
        bed_leveling: false,
        flow_cali: false,
        auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
        auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
        timelapse: false,
        ams_mapping_json: None,
        ams_mapping2_json: None,
        ams_mapping_info_json: None,
    }
}

fn actor() -> AuditActor {
    AuditActor::tenant_token(None, "cancel-race", vec!["*"])
}
