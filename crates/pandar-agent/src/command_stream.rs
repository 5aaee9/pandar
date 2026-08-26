use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, anyhow};
use tokio::{sync::mpsc, task::JoinSet};
use tokio_stream::{Stream, StreamExt};
use tonic::Status;

use crate::{
    AgentConfig,
    backoff::RunOutcome,
    camera_control::handle_control_camera_command,
    commands::{
        handle_firmware_command, handle_non_firmware_command_with_gateway, is_firmware_command,
    },
    machine::{BambuMachineGateway, FirmwareMachineGateway},
};
use pandar_protocol::agent::v1::{AgentEvent, HubCommand, hub_command};

#[cfg(test)]
pub(super) async fn handle_command_stream_with_gateway<G, S>(
    config: &AgentConfig,
    gateway: Arc<G>,
    sender: &mpsc::Sender<AgentEvent>,
    commands: S,
    session_epoch: u64,
) -> anyhow::Result<RunOutcome>
where
    G: BambuMachineGateway + FirmwareMachineGateway + 'static,
    S: Stream<Item = Result<HubCommand, Status>> + Unpin,
{
    let mut outcome = run_command_stream_until_cancelled(
        config,
        Arc::clone(&gateway),
        sender,
        commands,
        session_epoch,
        std::future::pending(),
    )
    .await;
    if let Err(error) = gateway.cancel_firmware_session(session_epoch).await {
        retain_first_error(
            &mut outcome,
            error.context("teardown firmware MQTT tasks for command stream"),
        );
    }
    outcome
}

pub(super) async fn run_command_stream_until_cancelled<G, S, C>(
    config: &AgentConfig,
    gateway: Arc<G>,
    sender: &mpsc::Sender<AgentEvent>,
    mut commands: S,
    session_epoch: u64,
    cancellation: C,
) -> anyhow::Result<RunOutcome>
where
    G: BambuMachineGateway + FirmwareMachineGateway + 'static,
    S: Stream<Item = Result<HubCommand, Status>> + Unpin,
    C: std::future::Future<Output = ()>,
{
    tokio::pin!(cancellation);
    let camera_tasks = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let (normal_sender, mut normal_receiver) = mpsc::channel::<HubCommand>(32);
    let normal_config = config.clone();
    let normal_gateway = Arc::clone(&gateway);
    let normal_event_sender = sender.clone();
    let normal_camera_tasks = Arc::clone(&camera_tasks);
    let mut normal_worker = tokio::spawn(async move {
        while let Some(command) = normal_receiver.recv().await {
            if let Some(hub_command::Command::CameraStream(camera_command)) = command.command {
                let mut camera_tasks = normal_camera_tasks.lock().await;
                handle_control_camera_command(
                    &normal_config,
                    normal_gateway.as_ref(),
                    &mut camera_tasks,
                    camera_command,
                )
                .await?;
                continue;
            }
            handle_non_firmware_command_with_gateway(
                &normal_config,
                normal_gateway.as_ref(),
                &normal_event_sender,
                command,
            )
            .await?;
        }
        Ok::<_, anyhow::Error>(())
    });
    let mut normal_completed = false;
    let mut firmware_tasks = JoinSet::new();
    let mut outcome = loop {
        tokio::select! {
            command = commands.next() => match command {
                Some(Ok(command)) if is_firmware_command(&command) => {
                    if firmware_tasks.len() >= 4 {
                        let result = firmware_tasks
                            .join_next()
                            .await
                            .expect("bounded firmware task set is non-empty");
                        if let Err(error) = firmware_task_result(result) {
                            break Err(error);
                        }
                    }
                    let Some(firmware_command) = command.command else {
                        unreachable!("firmware command classifier requires a command");
                    };
                    let task_config = config.clone();
                    let task_gateway = Arc::clone(&gateway);
                    let task_sender = sender.clone();
                    firmware_tasks.spawn(async move {
                        handle_firmware_command(
                            &task_config,
                            task_gateway.as_ref(),
                            &task_sender,
                            command.command_id,
                            firmware_command,
                            session_epoch,
                        )
                        .await
                    });
                }
                Some(Ok(command)) => {
                    if normal_sender.send(command).await.is_err() {
                        break Err(anyhow!("normal Agent command worker ended"));
                    }
                }
                Some(Err(error)) => {
                    break Err(anyhow::Error::new(error))
                        .context("read hub command from reverse stream");
                }
                None => break Ok(RunOutcome::ConnectedThenEnded),
            },
            result = &mut normal_worker => {
                normal_completed = true;
                break match result {
                    Ok(Ok(())) => Err(anyhow!("normal Agent command worker ended unexpectedly")),
                    Ok(Err(error)) => Err(error).context("run normal Agent command worker"),
                    Err(error) => Err(error).context("join normal Agent command worker"),
                };
            }
            result = firmware_tasks.join_next(), if !firmware_tasks.is_empty() => {
                let result = result.expect("non-empty firmware task set has a completion");
                match firmware_task_result(result) {
                    Ok(()) => {}
                    Err(error) => break Err(error),
                }
            }
            _ = &mut cancellation => break Ok(RunOutcome::ConnectedThenEnded),
        }
    };

    firmware_tasks.abort_all();
    if !normal_completed {
        normal_worker.abort();
    }
    while let Some(result) = firmware_tasks.join_next().await {
        if let Err(error) = firmware_task_result(result) {
            retain_first_error(&mut outcome, error);
        }
    }
    if !normal_completed {
        match normal_worker.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                retain_first_error(
                    &mut outcome,
                    error.context("run normal Agent command worker during teardown"),
                );
            }
            Err(error) if error.is_cancelled() => {}
            Err(error) => retain_first_error(
                &mut outcome,
                anyhow::Error::new(error)
                    .context("join normal Agent command worker during teardown"),
            ),
        }
    }
    teardown_camera_tasks(&camera_tasks, &mut outcome).await;
    outcome
}

async fn teardown_camera_tasks<T>(
    camera_tasks: &tokio::sync::Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    outcome: &mut anyhow::Result<T>,
) {
    loop {
        let mut tasks = camera_tasks.lock().await;
        let Some(stream_id) = tasks.keys().next().cloned() else {
            return;
        };
        let task = tasks
            .get_mut(&stream_id)
            .expect("camera task key came from the same map");
        task.abort();
        #[cfg(test)]
        crate::camera_control::pause_camera_join_for_test(&stream_id).await;
        let result = (&mut *task).await;
        tasks.remove(&stream_id);
        drop(tasks);
        if let Err(error) = result
            && !error.is_cancelled()
        {
            retain_first_error(
                outcome,
                anyhow::Error::new(error).context("join camera task during teardown"),
            );
        }
    }
}

#[cfg(test)]
pub(crate) async fn teardown_camera_tasks_for_test(
    camera_tasks: &tokio::sync::Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
) -> anyhow::Result<()> {
    let mut outcome = Ok(());
    teardown_camera_tasks(camera_tasks, &mut outcome).await;
    outcome
}

fn firmware_task_result(
    result: Result<anyhow::Result<()>, tokio::task::JoinError>,
) -> anyhow::Result<()> {
    match result {
        Ok(result) => result.context("run firmware command task"),
        Err(error) if error.is_cancelled() => Ok(()),
        Err(error) => Err(error).context("join firmware command task"),
    }
}

fn retain_first_error<T>(outcome: &mut anyhow::Result<T>, error: anyhow::Error) {
    if outcome.is_ok() {
        *outcome = Err(error);
    } else {
        tracing::warn!(
            error = %format!("{error:#}"),
            "additional Agent command teardown failure"
        );
    }
}
