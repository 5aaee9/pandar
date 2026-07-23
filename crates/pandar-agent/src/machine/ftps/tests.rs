use super::*;

#[test]
fn profile_caps_tls_for_known_aliases_only() {
    assert!(!FtpsProfile::for_model(None).cap_tls_1_2);
    assert!(!FtpsProfile::for_model(Some("P1S")).cap_tls_1_2);
    assert!(FtpsProfile::for_model(Some("P2S")).cap_tls_1_2);
    assert!(FtpsProfile::for_model(Some("N7")).cap_tls_1_2);
    assert!(FtpsProfile::for_model(Some("X2D")).cap_tls_1_2);
    assert!(FtpsProfile::for_model(Some("N6")).cap_tls_1_2);
}

#[test]
fn default_profile_builds_tls_config() {
    let config = bambu_lan_ftps_tls_config_for_default_profile();

    assert!(config.alpn_protocols.is_empty());
}

#[test]
fn p2s_profile_builds_tls_config() {
    let config = bambu_lan_ftps_tls_config(FtpsProfile::for_model(Some("P2S")));

    assert!(config.alpn_protocols.is_empty());
}

#[test]
fn upload_size_verification_accepts_exact_match() {
    assert!(matches!(
        verify_uploaded_size(42, Some(42), "Metadata/job.3mf").unwrap(),
        UploadVerification::Verified
    ));
}

#[test]
fn upload_size_verification_rejects_mismatch() {
    let err = verify_uploaded_size(42, Some(41), "Metadata/job.3mf").unwrap_err();
    let message = err.to_string();

    assert!(message.contains("Metadata/job.3mf"));
    assert!(message.contains("expected 42 bytes"));
    assert!(message.contains("server reported 41 bytes"));
}

#[test]
fn upload_size_verification_rejects_missing_size() {
    let err = verify_uploaded_size(42, None, "Metadata/job.3mf").unwrap_err();
    let message = err.to_string();

    assert!(message.contains("Metadata/job.3mf"));
    assert!(message.contains("server did not return SIZE"));
}

#[tokio::test]
async fn ftps_only_print_upload_rejects_emmc_policy_without_network_io() {
    let transfer = FtpsMachineFileTransfer::new(BambuPrinterEndpoint {
        host: "192.0.2.10".to_owned(),
        serial: "01S00EXAMPLE".to_owned(),
        access_code: "12345678".to_owned(),
        model: Some("X1 Carbon".to_owned()),
        name: Some("garage".to_owned()),
    });

    let error = transfer
        .upload_print(
            "job.gcode.3mf",
            b"abc",
            TransferProtectionMode::ProtectedData,
            PrintUploadPolicy {
                try_emmc_print: true,
            },
        )
        .await
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "FTPS-only transfer cannot honor try_emmc_print"
    );
}
