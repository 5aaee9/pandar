use super::*;

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
async fn fake_records_trait_boundary_operations() {
    let fake = FakeMachineFileTransfer::default();

    fake.list("/cache").await.unwrap();
    fake.download("/cache/job.3mf").await.unwrap();
    fake.upload("/cache/job.3mf", b"0123456789").await.unwrap();
    fake.delete("/cache/job.3mf").await.unwrap();

    assert_eq!(
        fake.recorded_requests(),
        vec![
            FileTransferRequest::list("/cache"),
            FileTransferRequest::download("/cache/job.3mf"),
            FileTransferRequest::upload("/cache/job.3mf", 10),
            FileTransferRequest::delete("/cache/job.3mf"),
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

    fake.upload("job.gcode.3mf", b"abc").await.unwrap();
    fake.upload_print("job.gcode.3mf", b"abc", disabled)
        .await
        .unwrap();
    fake.upload_print("job.gcode.3mf", b"abc", enabled)
        .await
        .unwrap();

    assert_eq!(
        fake.recorded_requests(),
        vec![
            FileTransferRequest::upload("job.gcode.3mf", 3),
            FileTransferRequest::print_upload("job.gcode.3mf", 3, disabled),
            FileTransferRequest::print_upload("job.gcode.3mf", 3, enabled),
        ]
    );
}
