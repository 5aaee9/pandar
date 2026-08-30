#[test]
fn shared_hub_http_client_explicitly_disables_reqwest_protocol_retries() {
    let source =
        std::fs::read_to_string(format!("{}/src/http/client.rs", env!("CARGO_MANIFEST_DIR")))
            .unwrap();

    assert_eq!(
        source.matches(".retry(reqwest::retry::never())").count(),
        1,
        "shared Hub HTTP client must explicitly disable reqwest protocol-NACK retries"
    );
}
