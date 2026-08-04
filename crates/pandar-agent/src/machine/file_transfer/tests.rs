use super::*;
use crate::machine::BambuPrinterEndpoint;
use TransferProtectionMode::ProtectedData as P;

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
fn constants_match_runtime_policy() {
    assert_eq!(BAMBU_FILE_TRANSFER_PORT, 990);
    assert_eq!(BAMBU_FILE_TRANSFER_USERNAME, "bblp");
    assert_eq!(BAMBU_FILE_TRANSFER_CHUNK_SIZE, 64 * 1024);
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
    fake.download("/cache/job.3mf", P).await.unwrap();
    fake.upload("/cache/job.3mf", b"0123456789", P)
        .await
        .unwrap();
    fake.delete("/cache/job.3mf", P).await.unwrap();

    assert_eq!(
        fake.recorded_requests(),
        vec![
            (P, FileTransferRequest::list("/cache")),
            (P, FileTransferRequest::download("/cache/job.3mf")),
            (P, FileTransferRequest::upload("/cache/job.3mf", 10)),
            (P, FileTransferRequest::delete("/cache/job.3mf")),
        ]
    );
}

#[tokio::test]
async fn generic_and_print_uploads_keep_distinct_emmc_policy() {
    let fake = FakeMachineFileTransfer::default();
    let disabled = PrintUploadPolicy {
        try_emmc_print: false,
    };
    let enabled = PrintUploadPolicy {
        try_emmc_print: true,
    };

    fake.upload("job.gcode.3mf", b"abc", P).await.unwrap();
    fake.upload_print("job.gcode.3mf", b"abc", P, disabled)
        .await
        .unwrap();
    fake.upload_print("job.gcode.3mf", b"abc", P, enabled)
        .await
        .unwrap();

    assert_eq!(
        fake.recorded_requests(),
        vec![
            (P, FileTransferRequest::upload("job.gcode.3mf", 3)),
            (
                P,
                FileTransferRequest::print_upload("job.gcode.3mf", 3, disabled)
            ),
            (
                P,
                FileTransferRequest::print_upload("job.gcode.3mf", 3, enabled)
            ),
        ]
    );
}

#[test]
fn attempt_order_never_downgrades_to_clear_data() {
    let cache = TransferModeCache::default();
    cache.store_success("192.0.2.10", P);

    assert_eq!(
        transfer_attempt_order(&ep(Some("X1 Carbon")), &cache),
        vec![P]
    );
    assert_eq!(
        transfer_attempt_order(&ep(Some("A1 Mini")), &TransferModeCache::default()),
        vec![P]
    );
    assert_eq!(
        transfer_attempt_order(&ep(Some("X1")), &TransferModeCache::default()),
        vec![P]
    );
}

#[tokio::test]
async fn protected_first_success_caches_protected_mode() {
    let endpoint = ep(Some("A1 Mini"));
    let cache = TransferModeCache::default();
    let fake = FakeMachineFileTransfer::default();
    let result = run_with_transfer_mode(&endpoint, &cache, |mode| {
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
async fn failed_protected_transfer_does_not_fallback_to_clear_data() {
    let a1 = ep(Some("A1"));
    let fallback_cache = TransferModeCache::default();
    let fallback = FakeMachineFileTransfer::with_protected_failure();

    run_with_transfer_mode(&a1, &fallback_cache, |mode| {
        let fallback = fallback.clone();
        async move { fallback.delete("/cache/job.3mf", mode).await }
    })
    .await
    .unwrap_err();

    assert_eq!(fallback.recorded_modes(), vec![P]);
    assert_eq!(fallback_cache.get("192.0.2.10"), None);

    let endpoint = ep(Some("A1 Mini"));
    let fake = FakeMachineFileTransfer::with_protected_failure();
    let cache = TransferModeCache::default();

    run_with_transfer_mode(&endpoint, &cache, |mode| {
        let fake = fake.clone();
        async move { fake.list("/cache", mode).await }
    })
    .await
    .unwrap_err();

    assert_eq!(fake.recorded_modes(), vec![P]);
    assert_eq!(cache.get("192.0.2.10"), None);
}

#[tokio::test]
async fn failed_protected_mode_is_not_cached() {
    let endpoint = ep(Some("A1 Mini"));
    let cache = TransferModeCache::default();
    let fake = FakeMachineFileTransfer::with_protected_failure();

    let err = run_with_transfer_mode(&endpoint, &cache, |mode| {
        let fake = fake.clone();
        async move { fake.upload("/cache/job.3mf", b"0123456789", mode).await }
    })
    .await
    .unwrap_err();
    let message = format!("{err:#}");

    assert!(message.contains("protected data transfer failed"));
    assert_eq!(cache.get("192.0.2.10"), None);
}
