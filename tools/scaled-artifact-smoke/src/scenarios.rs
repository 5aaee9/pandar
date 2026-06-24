use std::{collections::HashSet, sync::Arc, time::Duration};

use anyhow::{Context, ensure};
use axum::http::StatusCode;
use pandar_core::{CommandStatus, PrintStatus};
use pandar_hub::{
    AppState,
    artifacts::ArtifactStorage,
    db::Database,
    grpc::print_reports::handle_print_report,
    printer_events::PrinterEvent,
    repositories::CreatePrintJob,
    runtime::spawn_control_plane_ready,
    sessions::{AgentSession, SessionToken},
};
use tokio::sync::mpsc;

use crate::{
    fixture::{
        ARTIFACT_BYTES, SmokeFixture, SmokeWorld, report, report_input, seed_fixture,
        world_with_storage,
    },
    harness::HarnessConfig,
    http::{
        connect_ws_with_ticket, create_print_expect_status, create_print_through_multipart_route,
        create_ws_ticket, dequeue_print_command, download_artifact, download_artifact_route,
        next_ws_event_type, serve_hub,
    },
    storage::{FailingObjectStorage, FailureMode, SharedObjectStorage},
};
use sea_orm::{ConnectionTrait, Statement};

pub async fn artifact_dispatch_download(
    iteration: usize,
    config: &HarnessConfig,
) -> anyhow::Result<()> {
    let world = SmokeWorld::for_config(config).await?;
    let fixture = seed_fixture(&world.hub_a, &config.fixture_suffix("artifact", iteration, 0))
        .await?;
    let (_control_plane, ready) = spawn_control_plane_ready(world.hub_b.clone());
    ready
        .await
        .context("control plane readiness channel closed")?
        .context("hub B control plane failed to start")?;
    let (wake_sender, mut wake_receiver) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    world
        .hub_b
        .sessions()
        .register(agent_session(&fixture, wake_sender, close_sender))
        .await;

    create_print_through_multipart_route(&world.hub_a, &fixture).await?;
    tokio::time::timeout(Duration::from_secs(1), wake_receiver.recv())
        .await
        .context("agent wake did not converge to hub B")?
        .context("agent wake channel closed")?;

    let (command_id, print) = dequeue_print_command(&world.hub_b, &fixture).await?;
    ensure!(
        !print.artifact_download_path.trim().is_empty(),
        "PrintProjectFile is missing artifact_download_path"
    );
    ensure!(
        !print.storage_path.contains("pandar-spool"),
        "smoke command used a shared spool path"
    );
    let persisted = world
        .hub_b
        .commands()
        .get_for_tenant(fixture.tenant_id, command_id)
        .await?
        .context("expected persisted command after dispatch")?;
    ensure!(
        persisted.status == CommandStatus::Sent,
        "command was not marked sent"
    );

    download_artifact(
        &world.hub_b,
        &world,
        &fixture,
        &print.artifact_download_path,
    )
    .await?;
    concurrent_plugin_pressure(iteration, config).await?;
    Ok(())
}

pub async fn websocket_fanout(iteration: usize, config: &HarnessConfig) -> anyhow::Result<()> {
    let world = SmokeWorld::for_config(config).await?;
    let fixture =
        seed_fixture(&world.hub_a, &config.fixture_suffix("fanout", iteration, 0)).await?;
    create_print_through_multipart_route(&world.hub_a, &fixture).await?;
    let created_job = world
        .hub_a
        .jobs()
        .list_for_tenant(fixture.tenant_id)
        .await?
        .into_iter()
        .next()
        .context("expected created job")?;
    let tickets = futures_util::future::try_join_all(
        (0..config.concurrency)
            .map(|_| create_ws_ticket(&world.hub_a, fixture.tenant_id, &fixture.tenant_token)),
    )
    .await?;

    let (_control_plane, ready) = spawn_control_plane_ready(world.hub_b.clone());
    ready
        .await
        .context("control plane readiness channel closed")?
        .context("hub B control plane failed to start")?;
    let base_url = serve_hub(world.hub_b.clone()).await?;
    let mut sockets = futures_util::future::try_join_all(
        tickets
            .iter()
            .map(|ticket| connect_ws_with_ticket(&base_url, fixture.tenant_id, ticket)),
    )
    .await?;

    let printer = world
        .hub_a
        .printers()
        .get_for_tenant(fixture.tenant_id, &fixture.printer_id)
        .await?
        .context("expected printer")?;
    world
        .hub_a
        .publish_printer_event(fixture.tenant_id, PrinterEvent::PrinterSnapshot { printer })
        .await;
    handle_print_report(
        &world.hub_a,
        fixture.tenant_id,
        fixture.agent_id,
        report(
            &fixture,
            Some(created_job.job.id),
            Some(created_job.artifact.id),
            "RUNNING",
        ),
    )
    .await
    .map_err(|status| anyhow::anyhow!("print report failed: {status}"))?;

    for socket in &mut sockets {
        let mut seen = HashSet::new();
        seen.insert(next_ws_event_type(socket).await?);
        seen.insert(next_ws_event_type(socket).await?);
        ensure!(
            seen.contains("printer_snapshot") && seen.contains("job_progress"),
            "websocket subscriber did not receive both event types: {seen:?}"
        );
    }
    Ok(())
}

pub async fn restart_convergence(iteration: usize, config: &HarnessConfig) -> anyhow::Result<()> {
    let world = SmokeWorld::for_config(config).await?;
    let fixture =
        seed_fixture(&world.hub_a, &config.fixture_suffix("restart", iteration, 0)).await?;
    create_print_through_multipart_route(&world.hub_a, &fixture).await?;
    let restarted = world.restarted_state();
    let (command_id, _print) = dequeue_print_command(&restarted, &fixture).await?;
    let persisted = restarted
        .commands()
        .get_for_tenant(fixture.tenant_id, command_id)
        .await?
        .context("expected persisted command after restart dequeue")?;
    ensure!(
        persisted.status == CommandStatus::Sent,
        "restarted state did not mark command sent"
    );

    let ticket = create_ws_ticket(&world.hub_a, fixture.tenant_id, &fixture.tenant_token).await?;
    let base_url = serve_hub(restarted.clone()).await?;
    let mut socket = connect_ws_with_ticket(&base_url, fixture.tenant_id, &ticket).await?;
    let (_control_plane, ready) = spawn_control_plane_ready(restarted.clone());
    ready
        .await
        .context("control plane readiness channel closed")?
        .context("restarted control plane failed to start")?;
    let printer = restarted
        .printers()
        .get_for_tenant(fixture.tenant_id, &fixture.printer_id)
        .await?
        .context("expected printer after restart")?;
    world
        .hub_a
        .publish_printer_event(fixture.tenant_id, PrinterEvent::PrinterSnapshot { printer })
        .await;
    ensure!(
        next_ws_event_type(&mut socket).await? == "printer_snapshot",
        "restarted websocket did not receive printer snapshot"
    );
    Ok(())
}

pub async fn storage_failures(iteration: usize, _config: &HarnessConfig) -> anyhow::Result<()> {
    let temp = tempfile::tempdir().context("create storage failure temp dir")?;
    let storage = FailingObjectStorage::new(SharedObjectStorage::new(temp.path().join("objects"))?);
    let shared_storage: Arc<dyn ArtifactStorage> = Arc::new(storage.clone());
    let world = world_with_storage(shared_storage).await?;
    let fixture = seed_fixture(&world.hub_a, &format!("storage-{iteration}")).await?;

    storage.set_mode(FailureMode::Put);
    let status = create_print_expect_status(&world.hub_a, &fixture).await?;
    ensure!(
        status != StatusCode::CREATED,
        "put failure unexpectedly created print"
    );
    ensure!(
        world.hub_a.commands().count().await? == 0,
        "put failure created command row"
    );
    ensure!(
        table_count(&world.database, "jobs").await? == 0,
        "put failure created job row"
    );

    storage.set_mode(FailureMode::None);
    create_print_through_multipart_route(&world.hub_a, &fixture).await?;
    let (_, print) = dequeue_print_command(&world.hub_b, &fixture).await?;
    storage.set_mode(FailureMode::Open);
    let (status, body) =
        download_artifact_route(&world.hub_b, &fixture, &print.artifact_download_path).await?;
    ensure!(
        status == StatusCode::BAD_GATEWAY
            && String::from_utf8_lossy(&body).contains("artifact_unavailable"),
        "open failure did not surface artifact_unavailable: {status} {}",
        String::from_utf8_lossy(&body)
    );

    storage.set_mode(FailureMode::Delete);
    let delete_err = world
        .hub_a
        .artifact_storage()
        .delete_artifact(&print.storage_path)
        .await
        .expect_err("delete failure should reject delete");
    ensure!(
        format!("{delete_err:#}").contains("injected artifact delete failure"),
        "delete failure did not preserve context: {delete_err:#}"
    );
    Ok(())
}

async fn concurrent_plugin_pressure(
    iteration: usize,
    config: &HarnessConfig,
) -> anyhow::Result<()> {
    let world = SmokeWorld::for_config(config).await?;
    let mut fixtures = Vec::with_capacity(config.concurrency);
    for index in 0..config.concurrency {
        fixtures.push(
            seed_fixture(
                &world.hub_a,
                &config.fixture_suffix("pressure", iteration, index),
            )
            .await?,
        );
    }
    let limiter = Arc::new(tokio::sync::Semaphore::new(config.concurrency));
    futures_util::future::try_join_all(fixtures.iter().map(|fixture| {
        let limiter = limiter.clone();
        let hub = world.hub_a.clone();
        async move {
            let _permit = limiter.acquire_owned().await?;
            create_print_through_multipart_route(&hub, fixture).await
        }
    }))
    .await?;
    ensure!(
        world.hub_b.commands().count().await? == config.concurrency as i64,
        "expected one command per concurrent plugin client"
    );
    let mut prints = Vec::new();
    for fixture in &fixtures {
        prints.push(dequeue_print_command(&world.hub_b, fixture).await?.1);
    }
    ensure!(
        prints.len() == config.concurrency,
        "drained {} commands, expected {}",
        prints.len(),
        config.concurrency
    );
    if let Some(first) = prints.first() {
        let fixture = fixtures.first().context("expected first fixture")?;
        download_artifact(&world.hub_b, &world, fixture, &first.artifact_download_path).await?;
    }
    if let Some(last) = prints.last() {
        let fixture = fixtures.last().context("expected last fixture")?;
        download_artifact(&world.hub_b, &world, fixture, &last.artifact_download_path).await?;
    }
    Ok(())
}

pub async fn terminal_report_idempotence(
    iteration: usize,
    config: &HarnessConfig,
) -> anyhow::Result<()> {
    let world = SmokeWorld::for_config(config).await?;
    let fixture =
        seed_fixture(&world.hub_a, &config.fixture_suffix("terminal", iteration, 0)).await?;
    let created = create_print_job(&world.hub_a, &fixture).await?;
    let terminal = apply_report(
        &world.hub_a,
        &fixture,
        Some(created.job.id),
        Some(created.artifact.id.clone()),
        "FINISH",
    )
    .await?;
    let terminal_event_count = table_count(&world.database, "machine_events").await?;
    let replay = apply_report(
        &world.hub_a,
        &fixture,
        Some(created.job.id),
        Some(created.artifact.id.clone()),
        "FINISH",
    )
    .await?;
    ensure!(terminal.changed, "first terminal report did not change job");
    ensure!(!replay.changed, "terminal replay was not idempotent");
    ensure!(
        table_count(&world.database, "machine_events").await? == terminal_event_count,
        "terminal replay inserted another machine event"
    );

    apply_report(
        &world.hub_a,
        &fixture,
        Some(created.job.id),
        Some(created.artifact.id),
        "RUNNING",
    )
    .await?;
    let persisted = world
        .hub_a
        .jobs()
        .get_for_tenant(fixture.tenant_id, created.job.id)
        .await?
        .context("expected terminal job")?;
    ensure!(
        persisted.job.print.status == PrintStatus::Completed,
        "stale RUNNING report regressed terminal print status"
    );
    Ok(())
}

fn agent_session(
    fixture: &SmokeFixture,
    wake_sender: mpsc::Sender<()>,
    close_sender: mpsc::Sender<()>,
) -> AgentSession {
    let now = pandar_core::created_at_now();
    AgentSession {
        token: SessionToken::new(),
        tenant_id: fixture.tenant_id,
        agent_id: fixture.agent_id,
        name: "scaled-smoke-agent".to_owned(),
        version: "scaled-smoke".to_owned(),
        connected_at: now.clone(),
        last_heartbeat_at: now,
        wake_sender,
        close_sender,
    }
}

async fn create_print_job(
    state: &AppState,
    fixture: &SmokeFixture,
) -> anyhow::Result<pandar_hub::repositories::JobWithArtifact> {
    state
        .jobs()
        .create_print_job(CreatePrintJob {
            tenant_id: fixture.tenant_id,
            printer_id: fixture.printer_id.clone(),
            agent_id: fixture.agent_id,
            artifact_id: pandar_core::JobId::new().to_string(),
            artifact_filename: "terminal.3mf".to_owned(),
            artifact_content_type: "model/3mf".to_owned(),
            artifact_size_bytes: ARTIFACT_BYTES.len() as u64,
            artifact_storage_path: format!(
                "{}/{}/terminal.3mf",
                fixture.tenant_id,
                pandar_core::JobId::new()
            ),
            artifact_metadata_json: None,
            plate_id: 1,
            use_ams: true,
            flow_cali: false,
            timelapse: false,
            ams_mapping_json: None,
            ams_mapping2_json: None,
        })
        .await
        .map_err(anyhow::Error::from)
        .context("failed to create terminal print job")
}

async fn apply_report(
    state: &AppState,
    fixture: &SmokeFixture,
    job_id: Option<pandar_core::JobId>,
    artifact_id: Option<String>,
    gcode_state: &str,
) -> anyhow::Result<pandar_hub::repositories::AppliedPrintReport> {
    state
        .jobs()
        .apply_print_report(report_input(fixture, job_id, artifact_id, gcode_state))
        .await
        .map_err(anyhow::Error::from)
        .context("failed to apply print report")
}

async fn table_count(database: &Database, table: &'static str) -> anyhow::Result<i64> {
    let sql = format!("SELECT COUNT(*) AS count FROM {table}");
    let statement = Statement::from_string(
        match database.backend() {
            pandar_hub::db::DatabaseBackend::Sqlite => sea_orm::DatabaseBackend::Sqlite,
            pandar_hub::db::DatabaseBackend::Postgres => sea_orm::DatabaseBackend::Postgres,
        },
        sql,
    );
    let row = database
        .sea_orm_connection()
        .query_one_raw(statement)
        .await?
        .context("count query returned no row")?;
    row.try_get("", "count").map_err(anyhow::Error::from)
}
