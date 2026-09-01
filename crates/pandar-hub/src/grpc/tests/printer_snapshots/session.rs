use super::*;

#[tokio::test]
async fn grpc_printer_snapshot_persists_printer_state() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (_stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(snapshot_event(
            tenant_id,
            agent_id,
            snapshot(" SN-001 ", " X1 Carbon ", " X1C ", " idle "),
        )))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let printers = state.printers().list_for_tenant(tenant_id).await.unwrap();
    assert_eq!(printers.len(), 1);
    assert_eq!(printers[0].agent_id, agent_id);
    assert_eq!(printers[0].serial_number, "SN-001");
    assert_eq!(printers[0].name, "X1 Carbon");
    assert_eq!(printers[0].model.as_deref(), Some("X1C"));
    assert_eq!(printers[0].status, "idle");
    assert!(printers[0].last_seen_at.ends_with('Z'));
}

#[tokio::test]
async fn grpc_printer_device_features_preserve_presence_and_invalidate_session() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let token = register_test_session(&state, tenant_id, agent_id).await;
    let mut full = snapshot("FEATURE-SERIAL", "Feature Printer", "X2D", "idle");
    full.device_features = Some(PrinterDeviceFeatures {
        bambu_fun_bits: Some(0x8000_0041_0000_0020),
        bambu_fun2_bits: Some(0x8000_0000_0000_0021),
    });

    handle_event(
        &state,
        tenant_id,
        agent_id,
        token,
        snapshot_event(tenant_id, agent_id, full),
    )
    .await
    .unwrap();
    let printer = state
        .printers()
        .list_for_tenant(tenant_id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        printer.bambu_device_features,
        Some(pandar_core::BambuDeviceFeatures::from_bits(
            0x8000_0041_0000_0020
        ))
    );
    assert_eq!(
        printer.bambu_device_features_session_id,
        Some(token.persisted_id())
    );
    assert_eq!(
        printer.bambu_device_features2,
        Some(pandar_core::BambuDeviceFeatures::from_bits(
            0x8000_0000_0000_0021
        ))
    );
    assert_eq!(
        printer.bambu_device_features2_session_id,
        Some(token.persisted_id())
    );

    handle_event(
        &state,
        tenant_id,
        agent_id,
        token,
        snapshot_event(
            tenant_id,
            agent_id,
            snapshot("FEATURE-SERIAL", "Feature Printer", "X2D", "printing"),
        ),
    )
    .await
    .unwrap();
    let preserved = state
        .printers()
        .list_for_tenant(tenant_id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        preserved.bambu_device_features,
        Some(pandar_core::BambuDeviceFeatures::from_bits(
            0x8000_0041_0000_0020
        ))
    );
    assert_eq!(
        preserved.bambu_device_features_session_id,
        Some(token.persisted_id())
    );
    assert_eq!(
        preserved.bambu_device_features2,
        Some(pandar_core::BambuDeviceFeatures::from_bits(
            0x8000_0000_0000_0021
        ))
    );
    assert_eq!(
        preserved.bambu_device_features2_session_id,
        Some(token.persisted_id())
    );

    handle_event(
        &state,
        tenant_id,
        agent_id,
        token,
        device_features_event(
            tenant_id,
            agent_id,
            "FEATURE-SERIAL",
            Some(PrinterDeviceFeatures {
                bambu_fun_bits: Some(0),
                bambu_fun2_bits: None,
            }),
        ),
    )
    .await
    .unwrap();
    let zero = state
        .printers()
        .list_for_tenant(tenant_id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        zero.bambu_device_features,
        Some(pandar_core::BambuDeviceFeatures::default())
    );
    assert_eq!(
        zero.bambu_device_features_session_id,
        Some(token.persisted_id())
    );

    handle_event(
        &state,
        tenant_id,
        agent_id,
        token,
        device_features_event(tenant_id, agent_id, "FEATURE-SERIAL", None),
    )
    .await
    .unwrap();
    let invalidated = state
        .printers()
        .list_for_tenant(tenant_id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        invalidated.bambu_device_features,
        Some(pandar_core::BambuDeviceFeatures::default())
    );
    assert_eq!(invalidated.bambu_device_features_session_id, None);
}

#[tokio::test]
async fn grpc_printer_snapshot_rejects_empty_serial() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(snapshot_event(
            tenant_id,
            agent_id,
            snapshot(" ", "X1 Carbon", "X1C", "idle"),
        )))
        .await
        .unwrap();
    let err = stream.next().await.unwrap().unwrap_err();

    assert_eq!(err.code(), Code::InvalidArgument);
    assert!(
        state
            .printers()
            .list_for_tenant(tenant_id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn stale_replaced_stream_snapshot_does_not_mutate_printer_state() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (_old_stream, old_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let (_new_stream, _new_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    old_sender
        .send(Ok(snapshot_event(
            tenant_id,
            agent_id,
            snapshot("SN-STALE", "Stale Printer", "X1C", "idle"),
        )))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    assert!(
        state
            .printers()
            .list_for_tenant(tenant_id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn replacement_session_blocks_old_snapshot_commit() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (_old_stream, _old_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let old_token = state.sessions().get(agent_id).await.unwrap().token;
    let mut paused = crate::sessions::transition_pause::install_before(old_token);

    let old_state = state.clone();
    let old_write = tokio::spawn(async move {
        handle_event(
            &old_state,
            tenant_id,
            agent_id,
            old_token,
            snapshot_event(
                tenant_id,
                agent_id,
                snapshot("SN-RACE", "Stale Printer", "X1C", "printing"),
            ),
        )
        .await
    });
    paused.wait_until_reached().await;

    let (_replacement_stream, _replacement_sender) =
        connect_live(&state, vec![hello_event(tenant_id, agent_id)])
            .await
            .unwrap();
    paused.resume();
    old_write.await.unwrap().unwrap();

    assert!(
        state
            .printers()
            .list_for_tenant(tenant_id)
            .await
            .unwrap()
            .is_empty(),
        "the stale snapshot must be rejected before its database commit"
    );
    let current = state.sessions().get(agent_id).await.unwrap();
    let persisted = persisted_agent(&state, agent_id).await;
    assert_eq!(
        persisted.current_session_id,
        Some(current.token.persisted_id())
    );
    assert_eq!(persisted.status, AgentStatus::Online.as_str());
}

#[tokio::test]
async fn slow_snapshot_fanout_does_not_block_subsequent_snapshot_on_the_stream() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (_stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let mut pause = crate::grpc::printer_snapshots::snapshot_fanout_pause::install("SN-SLOW");

    sender
        .send(Ok(snapshot_event(
            tenant_id,
            agent_id,
            snapshot("SN-SLOW", "Slow Printer", "X1C", "printing"),
        )))
        .await
        .unwrap();
    pause.wait_until_reached().await;

    // The first snapshot has committed its aggregate and is now stuck in the
    // event fanout; a subsequent snapshot on the same stream must still apply.
    sender
        .send(Ok(snapshot_event(
            tenant_id,
            agent_id,
            snapshot("SN-FAST", "Fast Printer", "P1S", "idle"),
        )))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let printers = state.printers().list_for_tenant(tenant_id).await.unwrap();
            if printers
                .iter()
                .any(|printer| printer.serial_number == "SN-FAST")
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("subsequent snapshot must apply while the first snapshot fanout is paused");

    // The paused snapshot's current-session aggregate committed atomically
    // before its fanout stalled.
    let printers = state.printers().list_for_tenant(tenant_id).await.unwrap();
    assert!(
        printers
            .iter()
            .any(|printer| printer.serial_number == "SN-SLOW" && printer.status == "printing")
    );

    pause.resume();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let printers = state.printers().list_for_tenant(tenant_id).await.unwrap();
    assert_eq!(printers.len(), 2);
    assert!(
        state.sessions().get(agent_id).await.is_some(),
        "the stream must survive concurrent snapshot application"
    );
}

#[tokio::test]
async fn replacement_waits_for_snapshot_that_already_owns_transition_lease() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (_old_stream, _old_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let old_token = state.sessions().get(agent_id).await.unwrap().token;
    let mut paused = crate::sessions::transition_pause::install_after(old_token);

    let old_state = state.clone();
    let old_write = tokio::spawn(async move {
        handle_event(
            &old_state,
            tenant_id,
            agent_id,
            old_token,
            snapshot_event(
                tenant_id,
                agent_id,
                snapshot("SN-LINEARIZED", "Linearized Printer", "X1C", "printing"),
            ),
        )
        .await
    });
    paused.wait_until_reached().await;

    let replacement_state = state.clone();
    let replacement_token = SessionToken::new();
    let mut waiting = crate::sessions::transition_pause::observe_waiting(replacement_token);
    let replacement = tokio::spawn(async move {
        register_test_session_with_token(
            &replacement_state,
            tenant_id,
            agent_id,
            replacement_token,
        )
        .await;
        replacement_token
    });
    waiting.wait_until_reached().await;
    assert!(
        !replacement.is_finished(),
        "replacement must wait for the already-linearized snapshot"
    );

    paused.resume();
    old_write.await.unwrap().unwrap();
    let replacement = replacement.await.unwrap();

    let printers = state.printers().list_for_tenant(tenant_id).await.unwrap();
    assert_eq!(printers.len(), 1);
    assert_eq!(printers[0].serial_number, "SN-LINEARIZED");
    assert_eq!(printers[0].status, "printing");
    let persisted = persisted_agent(&state, agent_id).await;
    assert_eq!(
        persisted.current_session_id,
        Some(replacement.persisted_id())
    );
}
