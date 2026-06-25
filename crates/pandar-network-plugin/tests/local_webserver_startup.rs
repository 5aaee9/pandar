use std::{
    collections::BTreeSet,
    sync::{Arc, Barrier},
    thread,
};

use pandar_network_plugin::{
    PluginHttpResult, pandar_plugin_free_with_capacity, pandar_plugin_start_local_webserver,
};
use serde_json::Value;

fn body(result: PluginHttpResult) -> String {
    if result.body_ptr.is_null() || result.body_len == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) };
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap);
    body
}

fn start_local(web_url: &str, hub_url: &str) -> Value {
    let result = pandar_plugin_start_local_webserver(
        web_url.as_ptr(),
        web_url.len(),
        hub_url.as_ptr(),
        hub_url.len(),
        true,
        true,
    );
    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 200);
    serde_json::from_str(&body(result)).unwrap()
}

#[test]
fn concurrent_first_start_uses_one_local_webserver() {
    let barrier = Arc::new(Barrier::new(8));
    let handles = (0..8)
        .map(|index| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                start_local(
                    &format!("http://web-{index}.example.test"),
                    &format!("http://hub-{index}.example.test"),
                )
            })
        })
        .collect::<Vec<_>>();

    let starts = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    let base_urls = starts
        .iter()
        .map(|value| value["base_url"].as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();

    assert_eq!(base_urls.len(), 1);
    assert!(starts.iter().all(|value| {
        value["base_url"]
            .as_str()
            .unwrap()
            .starts_with("http://127.0.0.1:")
    }));
}
