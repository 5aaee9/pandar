#[test]
fn firmware_http_client_explicitly_disables_reqwest_protocol_retries() {
    let source = std::fs::read_to_string(format!(
        "{}/src/firmware/http.rs",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();

    assert_eq!(
        source.matches(".retry(reqwest::retry::never())").count(),
        1,
        "firmware HTTP client must explicitly disable reqwest protocol-NACK retries"
    );
}
