use super::*;

/// `127.0.0.2` is loopback with no listener, so every transport connect in
/// these tests fails fast with a deterministic refusal instead of racing any
/// hermetic peer bound to `127.0.0.1:6000`.
fn transfer(model: &str) -> BambuMachineFileTransfer {
    BambuMachineFileTransfer::new(BambuPrinterEndpoint {
        host: "127.0.0.2".to_owned(),
        serial: "test-bambu-v1".to_owned(),
        access_code: "12345678".to_owned(),
        model: Some(model.to_owned()),
        name: None,
    })
}

#[tokio::test]
async fn printable_upload_probes_brtc_for_any_model_before_ftps_fallback() {
    // The refused-port failure proves no model allowlist gates the eMMC
    // transport: the BRTC tunnel is attempted first for every model, then the
    // error degrades to FTPS while preserving the full BRTC cause chain.
    let error = transfer("A1 Mini")
        .upload_print(
            "Metadata/plate.gcode.3mf",
            b"abc",
            PrintUploadPolicy {
                try_emmc_print: false,
            },
        )
        .await
        .unwrap_err();
    let message = format!("{error:#}");

    assert!(
        message.contains("BRTC upload failed before FTPS fallback"),
        "{message}"
    );
    assert!(message.contains("connect Bambu BRTC tunnel"), "{message}");
    assert!(message.contains("connect implicit FTPS"), "{message}");
}

#[tokio::test]
async fn generic_printable_upload_shares_brtc_probe_and_ftps_fallback() {
    let error = transfer("P2S")
        .upload("plate.gcode.3mf", b"abc")
        .await
        .unwrap_err();
    let message = format!("{error:#}");

    assert!(message.contains("connect Bambu BRTC tunnel"), "{message}");
    assert!(message.contains("connect implicit FTPS"), "{message}");
}

#[tokio::test]
async fn non_model_uploads_skip_brtc_entirely() {
    let error = transfer("A1 Mini")
        .upload("Metadata/notes.txt", b"abc")
        .await
        .unwrap_err();
    let message = format!("{error:#}");

    assert!(message.contains("connect implicit FTPS"), "{message}");
    assert!(!message.contains("BRTC"), "{message}");
}
