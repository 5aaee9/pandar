use super::*;

#[test]
fn hub_url_normalization_allows_loopback_http() {
    assert_eq!(
        normalize_hub_url("http://localhost:3000/".to_owned()),
        Some("http://localhost:3000".to_owned())
    );
    assert_eq!(
        normalize_hub_url("http://127.0.0.1:8080/".to_owned()),
        Some("http://127.0.0.1:8080".to_owned())
    );
}
