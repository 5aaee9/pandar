use super::*;

#[test]
fn command_id_parse_round_trips_uuid_strings() {
    let id = CommandId::new();

    assert_eq!(CommandId::parse(&id.to_string()), Ok(id));
    assert_eq!(
        CommandId::parse("not-a-uuid"),
        Err(CoreError::InvalidCommandId)
    );
}

#[test]
fn command_status_round_trips_persisted_strings() {
    for (status, value) in [
        (CommandStatus::Queued, "queued"),
        (CommandStatus::Sent, "sent"),
        (CommandStatus::Acknowledged, "acknowledged"),
        (CommandStatus::Succeeded, "succeeded"),
        (CommandStatus::Failed, "failed"),
    ] {
        assert_eq!(status.as_str(), value);
        assert_eq!(value.parse::<CommandStatus>(), Ok(status));
    }
    assert_eq!(
        "lost".parse::<CommandStatus>(),
        Err(CoreError::InvalidCommandStatus("lost".to_string()))
    );
}

#[test]
fn command_record_from_parts_validates_kind_and_status() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let created_at = "2026-06-20T00:00:00Z";
    let build = |kind: &str, status: &str| {
        CommandRecord::from_parts(CommandRecordParts {
            id: CommandId::new(),
            tenant_id,
            agent_id,
            printer_id: None,
            kind: kind.to_owned(),
            status: status.to_owned(),
            payload_json: "{}".to_owned(),
            result_json: Some("{\"ok\":true}".to_owned()),
            error: None,
            created_at: created_at.to_owned(),
            updated_at: created_at.to_owned(),
        })
    };

    let record = build("refresh_printers", "queued").unwrap();
    assert_eq!(record.status, CommandStatus::Queued);
    assert_eq!(record.result_json.as_deref(), Some("{\"ok\":true}"));
    assert_eq!(
        build(" ", "queued").unwrap_err(),
        CoreError::EmptyCommandKind
    );
    assert_eq!(
        build("refresh_printers", "lost").unwrap_err(),
        CoreError::InvalidCommandStatus("lost".to_string())
    );
}

#[test]
fn job_status_round_trips_persisted_strings() {
    for (status, value) in [
        (JobStatus::Queued, "queued"),
        (JobStatus::Sent, "sent"),
        (JobStatus::Acknowledged, "acknowledged"),
        (JobStatus::Succeeded, "succeeded"),
        (JobStatus::Failed, "failed"),
    ] {
        assert_eq!(status.as_str(), value);
        assert_eq!(value.parse::<JobStatus>(), Ok(status));
    }
    assert_eq!(
        "printing".parse::<JobStatus>(),
        Err(CoreError::InvalidJobStatus("printing".to_string()))
    );
}

#[test]
fn print_status_round_trips_persisted_strings() {
    for (status, value) in [
        (PrintStatus::Pending, "pending"),
        (PrintStatus::Running, "running"),
        (PrintStatus::Completed, "completed"),
        (PrintStatus::Failed, "failed"),
        (PrintStatus::Cancelled, "cancelled"),
    ] {
        assert_eq!(status.as_str(), value);
        assert_eq!(value.parse::<PrintStatus>(), Ok(status));
    }
    assert_eq!(
        "paused".parse::<PrintStatus>(),
        Err(CoreError::InvalidPrintStatus("paused".to_string()))
    );
}

#[test]
fn job_print_state_pending_constructs_empty_pending_state() {
    let print = JobPrintState::pending();

    assert_eq!(print.status, PrintStatus::Pending);
    assert_eq!(print.progress_percent, None);
    assert_eq!(print.error, None);
}

#[test]
fn job_artifact_from_parts_validates_required_fields() {
    let tenant_id = TenantId::new();
    let created_at = "2026-06-22T00:00:00Z".to_string();
    let build = |id: &str, filename: &str, content_type: &str, size_bytes: u64, path: &str| {
        JobArtifact::from_parts(JobArtifactParts {
            id: id.to_string(),
            tenant_id,
            filename: filename.to_string(),
            content_type: content_type.to_string(),
            size_bytes,
            storage_path: path.to_string(),
            created_at: created_at.clone(),
        })
    };

    let artifact = build(
        "artifact-1",
        "plate.3mf",
        "model/3mf",
        42,
        "tenant/artifact.3mf",
    )
    .unwrap();
    assert_eq!(artifact.tenant_id, tenant_id);
    assert_eq!(artifact.size_bytes, 42);
    assert_eq!(
        build(" ", "plate.3mf", "model/3mf", 42, "tenant/artifact.3mf").unwrap_err(),
        CoreError::EmptyArtifactId
    );
    assert_eq!(
        build("artifact-1", " ", "model/3mf", 42, "tenant/artifact.3mf").unwrap_err(),
        CoreError::EmptyArtifactFilename
    );
    assert_eq!(
        build("artifact-1", "plate.3mf", " ", 42, "tenant/artifact.3mf").unwrap_err(),
        CoreError::EmptyArtifactContentType
    );
    assert_eq!(
        build(
            "artifact-1",
            "plate.3mf",
            "model/3mf",
            0,
            "tenant/artifact.3mf"
        )
        .unwrap_err(),
        CoreError::EmptyArtifactBody
    );
    assert_eq!(
        build("artifact-1", "plate.3mf", "model/3mf", 42, " ").unwrap_err(),
        CoreError::EmptyArtifactStoragePath
    );
}

#[test]
fn job_from_parts_validates_required_fields_and_status() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let command_id = CommandId::new();
    let created_at = "2026-06-22T00:00:00Z".to_string();
    let build = |printer_id: &str, artifact_id: &str, status: &str| {
        Job::from_parts(JobParts {
            id: JobId::new(),
            tenant_id,
            printer_id: printer_id.to_string(),
            agent_id,
            artifact_id: artifact_id.to_string(),
            command_id,
            status: status.to_string(),
            error: None,
            print_status: "pending".to_string(),
            printer_state: None,
            progress_percent: None,
            remaining_time_minutes: None,
            current_layer: None,
            total_layers: None,
            active_file: None,
            last_progress_percent: None,
            last_layer: None,
            print_error: None,
            print_started_at: None,
            print_finished_at: None,
            print_updated_at: None,
            created_at: created_at.clone(),
            updated_at: created_at.clone(),
        })
    };

    let job = build("printer-1", "artifact-1", "queued").unwrap();
    assert_eq!(job.tenant_id, tenant_id);
    assert_eq!(job.status, JobStatus::Queued);
    assert_eq!(
        build(" ", "artifact-1", "queued").unwrap_err(),
        CoreError::EmptyJobPrinterId
    );
    assert_eq!(
        build("printer-1", " ", "queued").unwrap_err(),
        CoreError::EmptyJobArtifactId
    );
    assert_eq!(
        build("printer-1", "artifact-1", "printing").unwrap_err(),
        CoreError::InvalidJobStatus("printing".to_string())
    );
}

#[test]
fn job_from_parts_rehydrates_print_state() {
    let created_at = "2026-06-22T00:00:00Z".to_string();
    let job = Job::from_parts(JobParts {
        id: JobId::new(),
        tenant_id: TenantId::new(),
        printer_id: "printer-1".to_string(),
        agent_id: AgentId::new(),
        artifact_id: "artifact-1".to_string(),
        command_id: CommandId::new(),
        status: "succeeded".to_string(),
        error: None,
        print_status: "running".to_string(),
        printer_state: Some("RUNNING".to_string()),
        progress_percent: Some(42),
        remaining_time_minutes: Some(15),
        current_layer: Some(3),
        total_layers: Some(9),
        active_file: Some("plate.3mf".to_string()),
        last_progress_percent: Some(42),
        last_layer: Some(3),
        print_error: None,
        print_started_at: Some("2026-06-22T00:01:00Z".to_string()),
        print_finished_at: None,
        print_updated_at: Some("2026-06-22T00:02:00Z".to_string()),
        created_at: created_at.clone(),
        updated_at: created_at,
    })
    .unwrap();

    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(job.print.status, PrintStatus::Running);
    assert_eq!(job.print.progress_percent, Some(42));
    assert_eq!(job.print.active_file.as_deref(), Some("plate.3mf"));
    assert_eq!(
        job.print.started_at.as_deref(),
        Some("2026-06-22T00:01:00Z")
    );
}

#[test]
fn job_from_parts_rejects_invalid_print_status() {
    let created_at = "2026-06-22T00:00:00Z".to_string();

    assert_eq!(
        Job::from_parts(JobParts {
            id: JobId::new(),
            tenant_id: TenantId::new(),
            printer_id: "printer-1".to_string(),
            agent_id: AgentId::new(),
            artifact_id: "artifact-1".to_string(),
            command_id: CommandId::new(),
            status: "queued".to_string(),
            error: None,
            print_status: "paused".to_string(),
            printer_state: None,
            progress_percent: None,
            remaining_time_minutes: None,
            current_layer: None,
            total_layers: None,
            active_file: None,
            last_progress_percent: None,
            last_layer: None,
            print_error: None,
            print_started_at: None,
            print_finished_at: None,
            print_updated_at: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        })
        .unwrap_err(),
        CoreError::InvalidPrintStatus("paused".to_string())
    );
}
