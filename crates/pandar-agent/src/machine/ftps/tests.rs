use super::*;

mod server;
use server::{DataConnectionPolicy, spawn_session_reuse_ftps_server, test_ftps_client_config};

#[test]
fn profile_caps_tls_for_known_aliases_only() {
    assert!(!FtpsProfile::for_model(None).cap_tls_1_2);
    assert!(!FtpsProfile::for_model(Some("P1S")).cap_tls_1_2);
    assert!(!FtpsProfile::for_model(Some("H2D")).cap_tls_1_2);
    assert!(FtpsProfile::for_model(Some("P2S")).cap_tls_1_2);
    assert!(FtpsProfile::for_model(Some("N7")).cap_tls_1_2);
    assert!(FtpsProfile::for_model(Some("X2D")).cap_tls_1_2);
    assert!(FtpsProfile::for_model(Some("Bambu Lab X2D")).cap_tls_1_2);
    assert!(FtpsProfile::for_model(Some("N6")).cap_tls_1_2);
}

#[test]
fn default_profile_builds_tls_config() {
    let config = bambu_lan_ftps_tls_config_for_default_profile();

    assert!(config.alpn_protocols.is_empty());
}

#[test]
fn p2s_profile_builds_tls_config() {
    let config = bambu_lan_ftps_tls_config(FtpsProfile::for_model(Some("P2S")), "test-printer");

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
async fn protected_upload_reuses_control_tls_session_for_passive_data() {
    let (address, server) =
        spawn_session_reuse_ftps_server(DataConnectionPolicy::RequireControlSessionId).await;
    let connector: AsyncRustlsConnector =
        tokio_rustls::TlsConnector::from(test_ftps_client_config(
            include_bytes!("../mqtt/tests/tls/bambu-v1-cert.pem"),
            FtpsProfile { cap_tls_1_2: true },
        ))
        .into();
    let mut stream = AsyncRustlsFtpStream::connect_secure_implicit(address, connector, "localhost")
        .await
        .unwrap();

    stream.login("bblp", "secret").await.unwrap();
    protect_data_channel(&mut stream).await.unwrap();
    let upload = upload_in_bambu_chunks(&mut stream, "job.3mf", b"abc").await;
    let size = if upload.is_ok() {
        Some(stream.size("job.3mf").await.unwrap())
    } else {
        None
    };
    let observed = server.await.unwrap();

    assert!(
        observed.data_session_reused,
        "passive data TLS performed a full handshake"
    );
    assert!(
        observed.control_session_id_reused,
        "passive data TLS did not reuse the control session ID"
    );
    upload.unwrap();
    assert_eq!(size, Some(3));
    assert_eq!(observed.uploaded, b"abc");
    assert_eq!(
        observed.commands,
        [
            "USER bblp",
            "PASS secret",
            "PBSZ 0",
            "PROT P",
            "TYPE I",
            "PASV",
            "STOR job.3mf",
            "SIZE job.3mf",
        ]
    );
}

#[tokio::test]
async fn x2d_policy_rejects_ticket_resumption_without_control_session_id_reuse() {
    let (address, server) =
        spawn_session_reuse_ftps_server(DataConnectionPolicy::RequireControlSessionId).await;
    let connector: AsyncRustlsConnector =
        tokio_rustls::TlsConnector::from(test_ftps_client_config(
            include_bytes!("../mqtt/tests/tls/bambu-v1-cert.pem"),
            FtpsProfile { cap_tls_1_2: false },
        ))
        .into();
    let mut stream = AsyncRustlsFtpStream::connect_secure_implicit(address, connector, "localhost")
        .await
        .unwrap();
    stream.login("bblp", "secret").await.unwrap();
    protect_data_channel(&mut stream).await.unwrap();

    let error = upload_in_bambu_chunks(&mut stream, "job.3mf", b"abc")
        .await
        .unwrap_err();
    let observed = server.await.unwrap();

    assert!(observed.data_session_reused);
    assert!(!observed.control_session_id_reused);
    assert!(format!("{error:#}").contains("522 SSL connection failed: session reuse required"));
}

#[tokio::test]
async fn h2d_default_profile_retains_protected_ticket_resumption() {
    let (address, server) =
        spawn_session_reuse_ftps_server(DataConnectionPolicy::RequireAnySessionReuse).await;
    let connector: AsyncRustlsConnector =
        tokio_rustls::TlsConnector::from(test_ftps_client_config(
            include_bytes!("../mqtt/tests/tls/bambu-v1-cert.pem"),
            FtpsProfile::for_model(Some("H2D")),
        ))
        .into();
    let mut stream = AsyncRustlsFtpStream::connect_secure_implicit(address, connector, "localhost")
        .await
        .unwrap();
    stream.login("bblp", "secret").await.unwrap();
    protect_data_channel(&mut stream).await.unwrap();

    upload_in_bambu_chunks(&mut stream, "job.3mf", b"abc")
        .await
        .unwrap();
    assert_eq!(stream.size("job.3mf").await.unwrap(), 3);
    let observed = server.await.unwrap();

    assert!(observed.data_session_reused);
    assert!(!observed.control_session_id_reused);
    assert_eq!(observed.uploaded, b"abc");
}

#[tokio::test]
async fn session_reuse_rejection_reports_data_connection_phase_and_complete_cause() {
    let (address, server) = spawn_session_reuse_ftps_server(DataConnectionPolicy::Reject).await;
    let connector: AsyncRustlsConnector =
        tokio_rustls::TlsConnector::from(test_ftps_client_config(
            include_bytes!("../mqtt/tests/tls/bambu-v1-cert.pem"),
            FtpsProfile { cap_tls_1_2: true },
        ))
        .into();
    let mut stream = AsyncRustlsFtpStream::connect_secure_implicit(address, connector, "localhost")
        .await
        .unwrap();
    stream.login("bblp", "secret").await.unwrap();
    protect_data_channel(&mut stream).await.unwrap();

    let error = upload_in_bambu_chunks(&mut stream, "job.3mf", b"abc")
        .await
        .unwrap_err();
    let observed = server.await.unwrap();

    assert!(observed.data_session_reused);
    assert_eq!(
        error.downcast_ref::<PrintTransferPhase>(),
        Some(&PrintTransferPhase::DataConnection)
    );
    assert!(format!("{error:#}").contains("522 SSL connection failed: session reuse required"));
    assert_eq!(
        observed.commands,
        [
            "USER bblp",
            "PASS secret",
            "PBSZ 0",
            "PROT P",
            "TYPE I",
            "PASV",
            "STOR job.3mf",
        ]
    );
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
