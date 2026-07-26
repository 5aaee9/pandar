use serde::Serialize;

pub(super) fn content_type(path: &str) -> &'static str {
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

pub(super) fn json_string<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("local webserver response is serializable")
}

#[derive(Serialize)]
pub(super) struct BaseUrlBody<'a> {
    pub(super) base_url: &'a str,
}

#[derive(Serialize)]
pub(super) struct StartBody<'a> {
    pub(super) base_url: &'a str,
    pub(super) web_url: &'a str,
    pub(super) hub_url: &'a str,
    pub(super) using_default_server: bool,
    pub(super) using_default_web_server: bool,
    pub(super) using_default_hub_server: bool,
}

#[derive(Serialize)]
pub(super) struct ConfigBody<'a> {
    pub(super) web_url: &'a str,
    pub(super) hub_url: &'a str,
    pub(super) using_default_server: bool,
    pub(super) using_default_web_server: bool,
    pub(super) using_default_hub_server: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HttpConfigBody<'a> {
    pub(super) web_url: &'a str,
    pub(super) hub_url: &'a str,
    pub(super) using_default_server: bool,
    pub(super) using_default_web_server: bool,
    pub(super) using_default_hub_server: bool,
    pub(super) config_nonce: &'a str,
    pub(super) callback_url: String,
}
