use super::*;
use crate::machine::BambuPrinterEndpoint;
use TransferProtectionMode::{ClearData as C, ProtectedData as P};

fn ep(model: Option<&str>) -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "192.0.2.10".to_string(),
        serial: "01S00EXAMPLE".to_string(),
        access_code: "12345678".to_string(),
        model: model.map(str::to_string),
        name: Some("garage-a1".to_string()),
    }
}

#[test]
fn constants_and_a1_detection_match_reference_policy() {
    assert_eq!(BAMBU_FILE_TRANSFER_PORT, 990);
    assert_eq!(BAMBU_FILE_TRANSFER_USERNAME, "bblp");
    assert_eq!(BAMBU_FILE_TRANSFER_CHUNK_SIZE, 64 * 1024);
    assert!(is_a1_model(Some("A1")));
    assert!(is_a1_model(Some("A1 Mini")));
    assert!(is_a1_model(Some("bambu lab a1 mini")));
    assert!(!is_a1_model(Some("P1S")));
    assert!(!is_a1_model(Some("X1 Carbon")));
    assert!(!is_a1_model(None));
}

#[test]
fn request_constructors_preserve_operation_shapes() {
    let requests = [
        (
            FileTransferRequest::list("/cache"),
            FileTransferOperation::List,
            "/cache",
        ),
        (
            FileTransferRequest::download("/cache/job.3mf"),
            FileTransferOperation::Download,
            "/cache/job.3mf",
        ),
        (
            FileTransferRequest::upload("/cache/job.3mf", 42),
            FileTransferOperation::Upload { size_bytes: 42 },
            "/cache/job.3mf",
        ),
        (
            FileTransferRequest::delete("/cache/job.3mf"),
            FileTransferOperation::Delete,
            "/cache/job.3mf",
        ),
    ];

    for (request, operation, path) in requests {
        assert_eq!(
            (request.operation, request.path.as_str()),
            (operation, path)
        );
    }
}

#[tokio::test]
async fn fake_records_trait_boundary_operations_and_modes() {
    let fake = FakeMachineFileTransfer::default();

    fake.list("/cache", P).await.unwrap();
    fake.download("/cache/job.3mf", C).await.unwrap();
    fake.upload("/cache/job.3mf", b"0123456789", P)
        .await
        .unwrap();
    fake.delete("/cache/job.3mf", C).await.unwrap();

    assert_eq!(
        fake.recorded_requests(),
        vec![
            (P, FileTransferRequest::list("/cache")),
            (C, FileTransferRequest::download("/cache/job.3mf")),
            (P, FileTransferRequest::upload("/cache/job.3mf", 10)),
            (C, FileTransferRequest::delete("/cache/job.3mf")),
        ]
    );
}

#[test]
fn attempt_order_uses_cache_force_clear_and_model_policy() {
    let cache = TransferModeCache::default();
    cache.store_success("192.0.2.10", C);

    assert_eq!(
        transfer_attempt_order(&ep(Some("X1 Carbon")), &cache, false),
        vec![C]
    );
    assert_eq!(
        transfer_attempt_order(&ep(Some("X1 Carbon")), &TransferModeCache::default(), true),
        vec![C]
    );
    assert_eq!(
        transfer_attempt_order(&ep(Some("A1 Mini")), &TransferModeCache::default(), false),
        vec![P, C]
    );
    assert_eq!(
        transfer_attempt_order(&ep(Some("X1")), &TransferModeCache::default(), false),
        vec![P]
    );
}

#[tokio::test]
async fn protected_first_success_caches_protected_mode() {
    let endpoint = ep(Some("A1 Mini"));
    let cache = TransferModeCache::default();
    let fake = FakeMachineFileTransfer::default();
    let result = run_with_transfer_mode(&endpoint, &cache, false, |mode| {
        let fake = fake.clone();
        async move { fake.list("/cache", mode).await }
    })
    .await
    .unwrap();

    assert_eq!(result, vec!["ok".to_string()]);
    assert_eq!(fake.recorded_modes(), vec![P]);
    assert_eq!(cache.get("192.0.2.10"), Some(P));
}

#[tokio::test]
async fn fallback_for_a1_forced_clear_and_cached_clear_use_clear_mode() {
    let a1 = ep(Some("A1"));
    let fallback_cache = TransferModeCache::default();
    let fallback = FakeMachineFileTransfer::with_failures(true, false);

    run_with_transfer_mode(&a1, &fallback_cache, false, |mode| {
        let fallback = fallback.clone();
        async move { fallback.delete("/cache/job.3mf", mode).await }
    })
    .await
    .unwrap();

    assert_eq!(fallback.recorded_modes(), vec![P, C]);
    assert_eq!(fallback_cache.get("192.0.2.10"), Some(C));

    let endpoint = ep(Some("A1 Mini"));
    let fake = FakeMachineFileTransfer::with_failures(true, false);
    let cache = TransferModeCache::default();

    run_with_transfer_mode(&endpoint, &cache, true, |mode| {
        let fake = fake.clone();
        async move { fake.list("/cache", mode).await }
    })
    .await
    .unwrap();

    assert_eq!(fake.recorded_modes(), vec![C]);
    assert_eq!(cache.get("192.0.2.10"), Some(C));

    let cached = FakeMachineFileTransfer::with_failures(true, false);
    run_with_transfer_mode(&endpoint, &cache, false, |mode| {
        let cached = cached.clone();
        async move { cached.download("/cache/job.3mf", mode).await }
    })
    .await
    .unwrap();

    assert_eq!(cached.recorded_modes(), vec![C]);
}

#[tokio::test]
async fn failed_modes_are_not_cached_and_combined_error_has_both_contexts() {
    let endpoint = ep(Some("A1 Mini"));
    let cache = TransferModeCache::default();
    let fake = FakeMachineFileTransfer::with_failures(true, true);

    let err = run_with_transfer_mode(&endpoint, &cache, false, |mode| {
        let fake = fake.clone();
        async move { fake.upload("/cache/job.3mf", b"0123456789", mode).await }
    })
    .await
    .unwrap_err();
    let message = format!("{err:#}");

    assert!(message.contains("protected data transfer failed"));
    assert!(message.contains("clear data transfer failed"));
    assert_eq!(cache.get("192.0.2.10"), None);
}
