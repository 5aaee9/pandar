use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::Duration,
};

use rust_embed::RustEmbed;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{PluginHttpResult, result, stable_error_body};

#[derive(RustEmbed)]
#[folder = "../../frontend/plugin-local/dist/"]
struct PluginLocalAssets;

static LOCAL_WEBSERVER: OnceLock<LocalWebserver> = OnceLock::new();
static LOCAL_WEBSERVER_INIT: Mutex<()> = Mutex::new(());

const MAX_LOCAL_HEADERS: usize = 16 * 1024;
const MAX_LOCAL_CONFIG_BODY: usize = 8 * 1024;
const LOCAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone)]
struct LocalWebserverConfig {
    web_url: String,
    hub_url: String,
    using_default_web_server: bool,
    using_default_hub_server: bool,
    user_selected: bool,
    config_nonce: String,
}

struct LocalWebserver {
    base_url: String,
    config: Arc<Mutex<LocalWebserverConfig>>,
}

struct LocalRequest {
    method: String,
    path: String,
    origin: Option<String>,
    body: String,
}

pub fn start(
    web_url: String,
    hub_url: String,
    web_configured: bool,
    hub_configured: bool,
) -> PluginHttpResult {
    let Some(web_url) = normalize_target_url(web_url) else {
        return invalid_target_server();
    };
    let Some(hub_url) = normalize_target_url(hub_url) else {
        return invalid_target_server();
    };
    let initial_config = LocalWebserverConfig {
        web_url,
        hub_url,
        using_default_web_server: !web_configured,
        using_default_hub_server: !hub_configured,
        user_selected: false,
        config_nonce: Uuid::new_v4().to_string(),
    };

    match start_or_update(initial_config) {
        Ok(server) => result(0, 200, start_body(server)),
        Err(_) => result(1, 0, stable_error_body("local_webserver_unavailable")),
    }
}

pub fn base_url() -> PluginHttpResult {
    match LOCAL_WEBSERVER.get() {
        Some(server) => result(0, 200, json!({ "base_url": server.base_url }).to_string()),
        None => result(1, 0, stable_error_body("local_webserver_unavailable")),
    }
}

pub fn config() -> PluginHttpResult {
    match LOCAL_WEBSERVER.get() {
        Some(server) => {
            let config = server
                .config
                .lock()
                .expect("local webserver config")
                .clone();
            result(0, 200, config_body(&config))
        }
        None => result(1, 0, stable_error_body("local_webserver_unavailable")),
    }
}

fn start_or_update(
    initial_config: LocalWebserverConfig,
) -> std::io::Result<&'static LocalWebserver> {
    if let Some(server) = LOCAL_WEBSERVER.get() {
        update_from_start(server, initial_config);
        return Ok(server);
    }
    let _guard = LOCAL_WEBSERVER_INIT.lock().expect("local webserver init");
    if let Some(server) = LOCAL_WEBSERVER.get() {
        update_from_start(server, initial_config);
        return Ok(server);
    }
    let server = bind_local_webserver(initial_config.clone())?;
    LOCAL_WEBSERVER
        .set(server)
        .map_err(|_| std::io::Error::other("local webserver already initialized"))?;
    let server = LOCAL_WEBSERVER
        .get()
        .expect("local webserver is set after successful bind");
    update_from_start(server, initial_config);
    Ok(server)
}

fn bind_local_webserver(config: LocalWebserverConfig) -> std::io::Result<LocalWebserver> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let base_url = format!("http://{}", listener.local_addr()?);
    let config = Arc::new(Mutex::new(config));
    let thread_config = Arc::clone(&config);
    let thread_base_url = base_url.clone();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let connection_config = Arc::clone(&thread_config);
            let connection_base_url = thread_base_url.clone();
            thread::spawn(move || {
                handle_local_connection(stream, connection_config, &connection_base_url);
            });
        }
    });
    Ok(LocalWebserver { base_url, config })
}

fn update_from_start(server: &LocalWebserver, incoming: LocalWebserverConfig) {
    let mut config = server.config.lock().expect("local webserver config");
    if config.user_selected {
        return;
    }
    *config = incoming;
}

fn start_body(server: &LocalWebserver) -> String {
    let config = server
        .config
        .lock()
        .expect("local webserver config")
        .clone();
    let using_default_server = config.using_default_web_server || config.using_default_hub_server;
    json!({
        "base_url": server.base_url,
        "web_url": config.web_url,
        "hub_url": config.hub_url,
        "using_default_server": using_default_server,
        "using_default_web_server": config.using_default_web_server,
        "using_default_hub_server": config.using_default_hub_server,
    })
    .to_string()
}

fn config_body(config: &LocalWebserverConfig) -> String {
    let using_default_server = config.using_default_web_server || config.using_default_hub_server;
    json!({
        "web_url": config.web_url,
        "hub_url": config.hub_url,
        "using_default_server": using_default_server,
        "using_default_web_server": config.using_default_web_server,
        "using_default_hub_server": config.using_default_hub_server,
    })
    .to_string()
}

fn http_config_body(base_url: &str, config: &LocalWebserverConfig) -> String {
    let using_default_server = config.using_default_web_server || config.using_default_hub_server;
    json!({
        "webUrl": config.web_url,
        "hubUrl": config.hub_url,
        "usingDefaultServer": using_default_server,
        "usingDefaultWebServer": config.using_default_web_server,
        "usingDefaultHubServer": config.using_default_hub_server,
        "configNonce": config.config_nonce,
        "callbackUrl": format!("{base_url}/callback"),
    })
    .to_string()
}

fn handle_local_connection(
    mut stream: TcpStream,
    config: Arc<Mutex<LocalWebserverConfig>>,
    base_url: &str,
) {
    let _ = stream.set_read_timeout(Some(LOCAL_REQUEST_TIMEOUT));
    let _ = stream.set_write_timeout(Some(LOCAL_REQUEST_TIMEOUT));
    let response = match read_local_request(&mut stream) {
        Ok(Some(request)) => route_local_request(request, config, base_url),
        Ok(None) => local_json_response(400, stable_error_body("bad_request")),
        Err(_) => local_json_response(400, stable_error_body("bad_request")),
    };
    let _ = stream.write_all(response.as_bytes());
}

fn route_local_request(
    request: LocalRequest,
    config: Arc<Mutex<LocalWebserverConfig>>,
    base_url: &str,
) -> String {
    let path = request
        .path
        .split('?')
        .next()
        .unwrap_or(request.path.as_str());
    if path.split('/').any(|segment| segment == "..") {
        return local_json_response(400, stable_error_body("bad_request"));
    }
    match (request.method.as_str(), path) {
        ("GET", "/sign-in") => local_asset_response("index.html"),
        ("GET", "/assets/app.js") => local_asset_response("assets/app.js"),
        ("GET", "/assets/styles.css") => local_asset_response("assets/styles.css"),
        ("GET", "/config") => {
            let config = config.lock().expect("local webserver config").clone();
            local_json_response(200, http_config_body(base_url, &config))
        }
        ("POST", "/config") => update_config(request, config, base_url),
        ("GET", "/callback") => local_html_response(
            200,
            "<!doctype html><html><body><main>Sign-in request received. Return to Studio.</main></body></html>",
        ),
        _ => local_json_response(404, stable_error_body("not_found")),
    }
}

fn update_config(
    request: LocalRequest,
    config: Arc<Mutex<LocalWebserverConfig>>,
    base_url: &str,
) -> String {
    if let Some(origin) = &request.origin
        && origin != base_url
    {
        return local_json_response(400, stable_error_body("bad_request"));
    }
    let Ok(body) = serde_json::from_str::<Value>(&request.body) else {
        return local_json_response(400, stable_error_body("invalid_target_server"));
    };
    let Some(web_url) = body
        .get("webUrl")
        .and_then(Value::as_str)
        .and_then(|value| normalize_target_url(value.to_owned()))
    else {
        return local_json_response(400, stable_error_body("invalid_target_server"));
    };
    let Some(hub_url) = body
        .get("hubUrl")
        .and_then(Value::as_str)
        .and_then(|value| normalize_target_url(value.to_owned()))
    else {
        return local_json_response(400, stable_error_body("invalid_target_server"));
    };

    let mut config = config.lock().expect("local webserver config");
    if body.get("configNonce").and_then(Value::as_str) != Some(config.config_nonce.as_str()) {
        return local_json_response(400, stable_error_body("bad_request"));
    }
    config.web_url = web_url;
    config.hub_url = hub_url;
    config.using_default_web_server = false;
    config.using_default_hub_server = false;
    config.user_selected = true;
    local_json_response(200, http_config_body(base_url, &config))
}

fn read_local_request(stream: &mut TcpStream) -> std::io::Result<Option<LocalRequest>> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    let headers_end = loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Ok(None);
        }
        request.extend_from_slice(&buffer[..read]);
        if request.len() > MAX_LOCAL_HEADERS {
            return Ok(None);
        }
        if let Some(pos) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let headers = String::from_utf8_lossy(&request[..headers_end]).to_string();
    let mut lines = headers.lines();
    let Some(request_line) = lines.next() else {
        return Ok(None);
    };
    let mut request_parts = request_line.split_whitespace();
    let Some(method) = request_parts.next() else {
        return Ok(None);
    };
    let Some(path) = request_parts.next() else {
        return Ok(None);
    };
    let method = method.to_owned();
    let path = path.to_owned();
    let content_length = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    });
    let origin = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("origin")
            .then(|| value.trim().to_owned())
    });

    let body_len = content_length.unwrap_or(0);
    if body_len > MAX_LOCAL_CONFIG_BODY {
        return Ok(None);
    }
    while request.len() - headers_end < body_len {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Ok(None);
        }
        request.extend_from_slice(&buffer[..read]);
    }
    let body = String::from_utf8_lossy(&request[headers_end..headers_end + body_len]).to_string();
    Ok(Some(LocalRequest {
        method,
        path,
        origin,
        body,
    }))
}

fn local_asset_response(path: &str) -> String {
    match PluginLocalAssets::get(path) {
        Some(asset) => local_response(200, content_type(path), asset.data.as_ref()),
        None => local_json_response(404, stable_error_body("not_found")),
    }
}

fn local_html_response(status: u16, body: &str) -> String {
    local_response(status, "text/html; charset=utf-8", body.as_bytes())
}

fn local_json_response(status: u16, body: String) -> String {
    local_response(status, "application/json; charset=utf-8", body.as_bytes())
}

fn local_response(status: u16, content_type: &str, body: &[u8]) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    let body = String::from_utf8_lossy(body);
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn content_type(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".json") {
        "application/json; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

fn normalize_target_url(value: String) -> Option<String> {
    let value = value.trim().trim_end_matches('/').to_string();
    if value.is_empty() {
        return None;
    }
    let url = reqwest::Url::parse(&value).ok()?;
    if matches!(url.scheme(), "http" | "https") && url.host_str().is_some() {
        Some(value)
    } else {
        None
    }
}

fn invalid_target_server() -> PluginHttpResult {
    result(1, 400, stable_error_body("invalid_target_server"))
}
