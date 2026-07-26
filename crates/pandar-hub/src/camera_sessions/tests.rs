use futures_util::StreamExt;
use pandar_core::{AgentId, TenantId};

use super::*;

#[tokio::test]
async fn open_stream_sends_agent_command_and_forwards_chunks() {
    let registry = CameraSessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let (command_sender, mut command_receiver) = mpsc::channel(1);

    let mut stream = registry
        .open_stream(tenant_id, agent_id, "SERIAL-1".to_owned(), command_sender)
        .await
        .unwrap();
    let command = command_receiver.recv().await.unwrap().unwrap();
    assert_eq!(command.command_id, stream.stream_id);
    match command.command.unwrap() {
        hub_command::Command::CameraStream(command) => match command.command.unwrap() {
            hub_camera_command::Command::Open(open) => {
                assert_eq!(command.stream_id, stream.stream_id);
                assert_eq!(open.serial_number, "SERIAL-1");
                assert_eq!(open.mode, CameraStreamMode::FragmentedMp4 as i32);
            }
            other => panic!("expected open camera command, got {other:?}"),
        },
        other => panic!("expected camera stream command, got {other:?}"),
    }

    registry
        .push_chunk(
            agent_id,
            &stream.stream_id.clone(),
            Bytes::from_static(b"frame"),
        )
        .await;
    let chunk = stream.next().await.unwrap().unwrap();
    assert_eq!(chunk, Bytes::from_static(b"frame"));
}

#[tokio::test]
async fn open_stream_replaces_existing_stream_for_same_printer() {
    let registry = CameraSessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let (command_sender, mut command_receiver) = mpsc::channel(4);

    let mut first = registry
        .open_stream(
            tenant_id,
            agent_id,
            "SERIAL-1".to_owned(),
            command_sender.clone(),
        )
        .await
        .unwrap();
    let first_open = camera_command(command_receiver.recv().await.unwrap().unwrap());

    let second = registry
        .open_stream(tenant_id, agent_id, "SERIAL-1".to_owned(), command_sender)
        .await
        .unwrap();
    let close = camera_command(command_receiver.recv().await.unwrap().unwrap());
    let second_open = camera_command(command_receiver.recv().await.unwrap().unwrap());

    assert_eq!(close.stream_id, first_open.stream_id);
    assert!(matches!(
        close.command,
        Some(hub_camera_command::Command::Close(_))
    ));
    assert_eq!(second_open.stream_id, second.stream_id);
    assert!(first.next().await.unwrap().is_err());
}

#[tokio::test]
async fn full_replaced_stream_is_nonblocking_and_counts_toward_capacity() {
    let registry = CameraSessionRegistry::with_max_streams_per_tenant(2);
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let (command_sender, mut command_receiver) = mpsc::channel(4);
    let first = registry
        .open_stream(
            tenant_id,
            agent_id,
            "SERIAL-1".to_owned(),
            command_sender.clone(),
        )
        .await
        .unwrap();
    command_receiver.recv().await.unwrap().unwrap();
    for _ in 0..16 {
        registry
            .push_chunk(agent_id, &first.stream_id, Bytes::from_static(b"frame"))
            .await;
    }

    let _second = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        registry.open_stream(
            tenant_id,
            agent_id,
            "SERIAL-1".to_owned(),
            command_sender.clone(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(matches!(
        registry
            .open_stream(
                tenant_id,
                agent_id,
                "SERIAL-2".to_owned(),
                command_sender.clone()
            )
            .await,
        Err(CameraOpenError::Capacity)
    ));

    drop(first);
    assert!(
        registry
            .open_stream(tenant_id, agent_id, "SERIAL-2".to_owned(), command_sender)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn close_agent_terminates_existing_camera_streams() {
    let registry = CameraSessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let (command_sender, mut command_receiver) = mpsc::channel(4);
    let mut stream = registry
        .open_stream(tenant_id, agent_id, "SERIAL-1".to_owned(), command_sender)
        .await
        .unwrap();
    let open = camera_command(command_receiver.recv().await.unwrap().unwrap());

    registry.close_agent(agent_id).await;

    let close = camera_command(command_receiver.recv().await.unwrap().unwrap());
    assert_eq!(close.stream_id, open.stream_id);
    assert!(matches!(
        close.command,
        Some(hub_camera_command::Command::Close(_))
    ));
    assert!(stream.next().await.unwrap().is_err());
}

fn camera_command(command: HubCommand) -> HubCameraCommand {
    match command.command.unwrap() {
        hub_command::Command::CameraStream(command) => command,
        other => panic!("expected camera stream command, got {other:?}"),
    }
}
