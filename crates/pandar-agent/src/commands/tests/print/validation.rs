use super::*;

#[tokio::test]
async fn missing_artifact_download_path_is_rejected_before_artifact_io() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakePrintGateway::ok(["SERIAL1"]);
    let reader = FakeArtifactReader::default();
    let (sender, mut receiver) = mpsc::channel(2);
    let mut command = print_command(command_id.clone(), "SERIAL1", "unused-storage-path");
    let Some(hub_command::Command::PrintProjectFile(print)) = &mut command.command else {
        panic!("expected print command");
    };
    print.artifact_download_path.clear();

    handle_command_with_reader(&config, &gateway, &reader, &sender, command)
        .await
        .unwrap();
    drop(sender);

    let event = receiver.recv().await.unwrap();
    let agent_event::Event::CommandAck(ack) = event.event.unwrap() else {
        panic!("expected rejected command ack");
    };
    assert!(!ack.accepted);
    assert!(ack.error.contains("missing artifact_download_path"));
    assert!(receiver.recv().await.is_none());
    assert!(reader.reads.lock().await.is_empty());
    assert!(gateway.prints.lock().await.is_empty());
}

#[tokio::test]
async fn corrupt_typed_command_is_rejected_before_artifact_io() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakePrintGateway::ok(["SERIAL1"]);
    let reader = FakeArtifactReader::with_artifacts([(ARTIFACT_DOWNLOAD_PATH, b"abc".to_vec())]);
    let (sender, mut receiver) = mpsc::channel(2);
    let mut command = print_command(command_id.clone(), "SERIAL1", "tenant/artifact/plate.3mf");
    let Some(hub_command::Command::PrintProjectFile(print)) = &mut command.command else {
        panic!("expected print command");
    };
    print.options.as_mut().unwrap().ams_mapping = vec![0; 33];

    handle_command_with_reader(&config, &gateway, &reader, &sender, command)
        .await
        .unwrap();
    drop(sender);

    let event = receiver.recv().await.unwrap();
    let agent_event::Event::CommandAck(ack) = event.event.unwrap() else {
        panic!("expected rejected command ack");
    };
    assert!(!ack.accepted);
    assert!(ack.error.contains("validate print-project-file command"));
    assert!(ack.error.contains("invalid ams_mapping"));
    assert!(receiver.recv().await.is_none());
    assert!(reader.reads.lock().await.is_empty());
    assert!(gateway.prints.lock().await.is_empty());
}
