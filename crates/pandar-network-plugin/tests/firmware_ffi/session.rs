use serde_json::json;

use super::{
    abi::Session,
    support::{Response, command, mock_hub, printer_batch, probe_hub},
};

#[test]
fn firmware_ffi_expected_generation_mismatch_performs_zero_hub_io() {
    let (hub_a, server_a) = probe_hub(Vec::new());
    let (hub_b, server_b) = probe_hub(Vec::new());
    let session = Session::create(&hub_a, "token-a", 1);
    assert_eq!(session.update(&hub_b, "token-b", 2), 0);

    let _catalog = session.catalog("SERIAL-A", "printer-a", 1);
    let _refresh = session.refresh("SERIAL-A", "printer-a", "stale-refresh", 1);
    let mut callback_token = 99;
    let _send = session.send(
        "SERIAL-A",
        "printer-a",
        &command("stale-send"),
        0,
        Some(&mut callback_token),
        1,
    );

    assert_eq!(callback_token, 0);
    session.destroy();
    let requests_a = server_a.join().unwrap();
    let requests_b = server_b.join().unwrap();
    assert!(
        requests_a.is_empty() && requests_b.is_empty(),
        "stale generation paired printer A with a Hub credential: A={requests_a:?} B={requests_b:?}"
    );
}

#[test]
fn firmware_ffi_session_read_surfaces_free_every_http_allocation() {
    let catalog_response = json!({
        "firmware":{"module_revision":8,"status_revision":9},
        "catalog":[{
            "target":"printer","version":"01.02.04.00",
            "url":"printer.bin","description":"Printer release"
        }]
    });
    let refresh_response = json!({
        "command_id":"00000000-0000-0000-0000-000000000001",
        "modules":[{"name":"ota","sw_ver":"01.02.03.04"}],
        "module_revision":8
    });
    let (hub, server) = mock_hub(vec![
        Response::json("200 OK", catalog_response.to_string()),
        Response::json("200 OK", refresh_response.to_string()),
    ]);
    let session = Session::create(&hub, "old-token", 1);

    assert_eq!(session.update(&hub, "new-token", 2), 0);
    assert_eq!(session.observe(printer_batch(), 2, 1), 0);

    let status = session.next_status("SERIAL");
    assert_eq!((status.status, status.http_code), (0, 200));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&status.body).unwrap()["info"]["module"][0]["name"],
        "ota"
    );

    let catalog = session.catalog("SERIAL", "printer-1", 2);
    assert_eq!((catalog.status, catalog.http_code), (0, 200));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&catalog.body).unwrap()["devices"][0]["firmware"]
            [0]["url"],
        "printer.bin"
    );

    let refresh = session.refresh("SERIAL", "printer-1", "0009", 2);
    assert_eq!((refresh.status, refresh.http_code), (0, 200));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&refresh.body).unwrap()["info"]["sequence_id"],
        "0009"
    );

    session.destroy();
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| {
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer new-token")
    }));
}

#[test]
fn firmware_ffi_observation_sequence_rejects_delayed_batch_and_allows_newer_recovery() {
    let session = Session::create("http://127.0.0.1:1", "token", 2);
    assert_eq!(session.observe(printer_batch(), 2, 10), 0);

    let mut invalidated = serde_json::from_str::<serde_json::Value>(printer_batch()).unwrap();
    invalidated["devices"][0]["firmware"] = serde_json::Value::Null;
    assert_eq!(session.observe(&invalidated.to_string(), 2, 12), 0);
    let reset = session.next_status("SERIAL");
    assert_eq!((reset.status, reset.http_code), (0, 200));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reset.body).unwrap()["info"]["result"],
        "fail"
    );

    assert_eq!(session.observe(printer_batch(), 2, 11), 0);
    let repeated_reset = session.next_status("SERIAL");
    assert_eq!((repeated_reset.status, repeated_reset.http_code), (0, 200));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&repeated_reset.body).unwrap()["info"]["result"],
        "fail"
    );

    assert_eq!(session.observe(printer_batch(), 2, 13), 0);
    let recovered = session.next_status("SERIAL");
    assert_eq!((recovered.status, recovered.http_code), (0, 200));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&recovered.body).unwrap()["info"]["module"][0]["name"],
        "ota"
    );
    session.destroy();
}

#[test]
fn firmware_ffi_observation_sequence_stays_monotonic_across_generation_update() {
    let hub = "http://127.0.0.1:1";
    let session = Session::create(hub, "token", 2);
    assert_eq!(session.observe(printer_batch(), 2, 10), 0);
    assert_eq!(session.update(hub, "token", 3), 0);

    assert_eq!(session.observe(printer_batch(), 3, 9), 0);
    let reset = session.next_status("SERIAL");
    assert_eq!((reset.status, reset.http_code), (0, 200));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reset.body).unwrap()["info"]["result"],
        "fail"
    );

    assert_eq!(session.observe(printer_batch(), 3, 11), 0);
    let recovered = session.next_status("SERIAL");
    assert_eq!((recovered.status, recovered.http_code), (0, 200));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&recovered.body).unwrap()["info"]["module"][0]["name"],
        "ota"
    );
    session.destroy();
}
