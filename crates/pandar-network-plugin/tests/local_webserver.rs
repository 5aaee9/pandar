use std::{
    io::{ErrorKind, Read, Write},
    net::TcpStream,
    thread,
    time::Duration,
};

use pandar_network_plugin::{
    PluginHttpResult, pandar_plugin_free_with_capacity, pandar_plugin_local_webserver_base_url,
    pandar_plugin_local_webserver_config, pandar_plugin_start_local_webserver,
};
use serde::Deserialize;

fn body(result: PluginHttpResult) -> String {
    if result.body_ptr.is_null() || result.body_len == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) };
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap);
    body
}

fn start_local(
    web_url: &str,
    hub_url: &str,
    web_configured: bool,
    hub_configured: bool,
) -> StartLocalResponse {
    let result = pandar_plugin_start_local_webserver(
        web_url.as_ptr(),
        web_url.len(),
        hub_url.as_ptr(),
        hub_url.len(),
        web_configured,
        hub_configured,
    );
    if result.status != 0 {
        panic!("start local webserver failed: {}", body(result));
    }
    assert_eq!(result.http_code, 200);
    serde_json::from_str(&body(result)).unwrap()
}

fn request(base_url: &str, request: String) -> String {
    let addr = base_url
        .strip_prefix("http://")
        .expect("local webserver uses http");
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    match stream.read_to_string(&mut response) {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::ConnectionReset && !response.is_empty() => {}
        Err(error) => panic!("read local webserver response: {error}"),
    }
    response
}

fn connect_idle(base_url: &str) -> TcpStream {
    let addr = base_url
        .strip_prefix("http://")
        .expect("local webserver uses http");
    TcpStream::connect(addr).unwrap()
}

fn get(base_url: &str, path: &str) -> String {
    request(
        base_url,
        format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"),
    )
}

fn post_json(base_url: &str, path: &str, body: &str) -> String {
    post_json_with_origin(base_url, path, body, None)
}

fn post_json_declared_length(base_url: &str, path: &str, declared_length: usize) -> String {
    request(
        base_url,
        format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {declared_length}\r\nConnection: close\r\n\r\n"
        ),
    )
}

fn post_json_with_origin(base_url: &str, path: &str, body: &str, origin: Option<&str>) -> String {
    let origin = origin
        .map(|origin| format!("Origin: {origin}\r\n"))
        .unwrap_or_default();
    request(
        base_url,
        format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\n{origin}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    )
}

fn response_body(response: &str) -> &str {
    response.split_once("\r\n\r\n").unwrap().1
}

fn response_json<T: for<'de> Deserialize<'de>>(response: &str) -> T {
    serde_json::from_str(response_body(response)).unwrap()
}

#[derive(Debug, Deserialize)]
struct StartLocalResponse {
    base_url: String,
    web_url: String,
    hub_url: String,
    using_default_server: bool,
    using_default_web_server: bool,
    using_default_hub_server: bool,
}

#[derive(Debug, Deserialize)]
struct BaseUrlResponse {
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct PluginConfigResponse {
    web_url: String,
    hub_url: String,
    using_default_server: bool,
    using_default_web_server: bool,
    using_default_hub_server: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserConfigResponse {
    web_url: String,
    hub_url: String,
    using_default_server: bool,
    using_default_web_server: bool,
    using_default_hub_server: bool,
    callback_url: String,
    config_nonce: String,
}

#[test]
fn local_webserver_serves_assets_rejects_bad_requests_and_switches_target_server() {
    let start = start_local(
        "http://localhost:3000/",
        "http://localhost:8080/",
        false,
        false,
    );
    let base_url = start.base_url.as_str();
    assert!(base_url.starts_with("http://127.0.0.1:"));
    assert_eq!(start.web_url, "http://localhost:3000");
    assert_eq!(start.hub_url, "http://localhost:8080");
    assert!(start.using_default_server);
    assert!(start.using_default_web_server);
    assert!(start.using_default_hub_server);

    let base_result = pandar_plugin_local_webserver_base_url();
    assert_eq!(base_result.status, 0);
    let base_response: BaseUrlResponse = serde_json::from_str(&body(base_result)).unwrap();
    assert_eq!(base_response.base_url, base_url);

    let sign_in = get(base_url, "/sign-in");
    assert!(sign_in.starts_with("HTTP/1.1 200 OK"));
    assert!(sign_in.contains("Content-Type: text/html; charset=utf-8"));
    assert!(sign_in.contains("Target server"));

    let script = get(base_url, "/assets/app.js");
    assert!(script.starts_with("HTTP/1.1 200 OK"));
    assert!(script.contains("Content-Type: application/javascript; charset=utf-8"));
    assert!(script.contains("hubUrl"));

    let styles = get(base_url, "/assets/styles.css");
    assert!(styles.starts_with("HTTP/1.1 200 OK"));
    assert!(styles.contains("Content-Type: text/css; charset=utf-8"));
    assert!(styles.contains(".panel"));

    let config: BrowserConfigResponse = response_json(&get(base_url, "/config"));
    assert_eq!(config.web_url, "http://localhost:3000");
    assert_eq!(config.hub_url, "http://localhost:8080");
    assert!(config.using_default_server);
    assert!(config.using_default_web_server);
    assert!(config.using_default_hub_server);
    assert_eq!(config.callback_url, format!("{base_url}/callback"));
    let config_nonce = config.config_nonce;

    let invalid_config = post_json(
        base_url,
        "/config",
        &format!(
            r#"{{"webUrl":"ftp://bad.example.test","hubUrl":"http://localhost:8080","configNonce":"{config_nonce}"}}"#
        ),
    );
    assert!(invalid_config.starts_with("HTTP/1.1 400 Bad Request"));
    assert_eq!(
        response_body(&invalid_config),
        r#"{"error":"invalid_target_server"}"#
    );
    let cleartext_remote_config = post_json(
        base_url,
        "/config",
        &format!(
            r#"{{"webUrl":"http://web.example.test","hubUrl":"https://hub.example.test","configNonce":"{config_nonce}"}}"#
        ),
    );
    assert!(cleartext_remote_config.starts_with("HTTP/1.1 400 Bad Request"));
    assert_eq!(
        response_body(&cleartext_remote_config),
        r#"{"error":"invalid_target_server"}"#
    );

    let missing = get(base_url, "/assets/missing.js");
    assert!(missing.starts_with("HTTP/1.1 404 Not Found"));
    assert_eq!(response_body(&missing), r#"{"error":"not_found"}"#);

    let traversal = get(base_url, "/../Cargo.toml");
    assert!(traversal.starts_with("HTTP/1.1 400 Bad Request"));
    assert_eq!(response_body(&traversal), r#"{"error":"bad_request"}"#);

    let large_body = post_json_declared_length(base_url, "/config", 9 * 1024);
    assert!(large_body.starts_with("HTTP/1.1 400 Bad Request"));
    assert_eq!(response_body(&large_body), r#"{"error":"bad_request"}"#);

    let missing_nonce = post_json(
        base_url,
        "/config",
        r#"{"webUrl":"https://web.example.test/","hubUrl":"https://hub.example.test/"}"#,
    );
    assert!(missing_nonce.starts_with("HTTP/1.1 400 Bad Request"));
    assert_eq!(response_body(&missing_nonce), r#"{"error":"bad_request"}"#);

    let wrong_origin = post_json_with_origin(
        base_url,
        "/config",
        &format!(
            r#"{{"webUrl":"https://web.example.test/","hubUrl":"https://hub.example.test/","configNonce":"{config_nonce}"}}"#
        ),
        Some("https://evil.example.test"),
    );
    assert!(wrong_origin.starts_with("HTTP/1.1 400 Bad Request"));
    assert_eq!(response_body(&wrong_origin), r#"{"error":"bad_request"}"#);

    let idle = connect_idle(base_url);
    let response = get(base_url, "/config");
    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert_eq!(
        response_json::<BrowserConfigResponse>(&response).hub_url,
        "http://localhost:8080"
    );
    thread::sleep(Duration::from_millis(20));
    drop(idle);

    let updated: BrowserConfigResponse = response_json(&post_json(
        base_url,
        "/config",
        &format!(
            r#"{{"webUrl":"https://web.example.test/","hubUrl":"https://hub.example.test/","configNonce":"{config_nonce}"}}"#
        ),
    ));
    assert_eq!(updated.web_url, "https://web.example.test");
    assert_eq!(updated.hub_url, "https://hub.example.test");
    assert!(!updated.using_default_server);
    assert!(!updated.using_default_web_server);
    assert!(!updated.using_default_hub_server);

    let config_result = pandar_plugin_local_webserver_config();
    assert_eq!(config_result.status, 0);
    let plugin_config: PluginConfigResponse = serde_json::from_str(&body(config_result)).unwrap();
    assert_eq!(plugin_config.web_url, "https://web.example.test");
    assert_eq!(plugin_config.hub_url, "https://hub.example.test");
    assert!(!plugin_config.using_default_server);
    assert!(!plugin_config.using_default_web_server);
    assert!(!plugin_config.using_default_hub_server);

    let restarted = start_local(
        "https://ignored-web.test",
        "https://ignored-hub.test",
        true,
        true,
    );
    assert_eq!(restarted.base_url, base_url);
    assert_eq!(restarted.web_url, "https://web.example.test");
    assert_eq!(restarted.hub_url, "https://hub.example.test");

    let callback = get(base_url, "/callback?ticket=secret-ticket");
    assert!(callback.starts_with("HTTP/1.1 200 OK"));
    assert!(!callback.contains("secret-ticket"));

    let studio_callback = get(
        base_url,
        "/callback?ticket=secret-ticket&redirect_url=http%3A%2F%2F127.0.0.1%3A55545%2Fcallback",
    );
    assert!(studio_callback.starts_with("HTTP/1.1 200 OK"));
    assert!(studio_callback.contains("user_ticket_login"));
    assert!(studio_callback.contains("secret-ticket"));
}

#[test]
fn localized_sign_in_routes_serve_root_relative_assets_and_reject_invalid_paths() {
    let start = start_local(
        "http://localhost:3000/",
        "http://localhost:8080/",
        false,
        false,
    );
    let base_url = start.base_url.as_str();

    for path in ["/en/sign-in", "/en_GB/sign-in", "/zh-CN/sign-in"] {
        let response = get(base_url, path);
        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "{path} returned {response}"
        );
        assert!(response_body(&response).contains(r#"href="/assets/styles.css""#));
        assert!(response_body(&response).contains(r#"src="/assets/app.js""#));
    }

    for path in [
        "/1/sign-in",
        "/english/sign-in",
        "/en/sign-in/extra",
        "/en/assets/app.js",
        "/en/config",
        "/-/sign-in",
        "/中文/sign-in",
    ] {
        let response = get(base_url, path);
        assert!(
            response.starts_with("HTTP/1.1 404 Not Found"),
            "{path} returned {response}"
        );
        assert_eq!(response_body(&response), r#"{"error":"not_found"}"#);
    }

    let traversal = get(base_url, "/zh/../sign-in");
    assert!(traversal.starts_with("HTTP/1.1 400 Bad Request"));
    assert_eq!(response_body(&traversal), r#"{"error":"bad_request"}"#);

    let post_sign_in = post_json(base_url, "/en/sign-in", "");
    assert!(post_sign_in.starts_with("HTTP/1.1 404 Not Found"));
    assert_eq!(response_body(&post_sign_in), r#"{"error":"not_found"}"#);
}
