use super::*;

#[test]
fn parses_printer_config_json() {
    let printers = parse_printer_config(
        r#"[{"host":"192.0.2.10","serial":"01S00EXAMPLE","access_code":"12345678","model":"A1 Mini","name":"garage-a1"}]"#,
    )
    .unwrap();

    assert_eq!(
        printers,
        vec![BambuPrinterEndpoint {
            host: "192.0.2.10".to_owned(),
            serial: "01S00EXAMPLE".to_owned(),
            access_code: "12345678".to_owned(),
            model: Some("A1 Mini".to_owned()),
            name: Some("garage-a1".to_owned()),
        }]
    );
}

#[test]
fn parses_printer_config_empty_config_means_no_printers() {
    assert_eq!(parse_printer_config("").unwrap(), Vec::new());
    assert_eq!(parse_printer_config("   ").unwrap(), Vec::new());
    assert_eq!(parse_printer_config("[]").unwrap(), Vec::new());
    assert_eq!(parse_printer_config("  []  ").unwrap(), Vec::new());
}

#[test]
fn invalid_printer_config_malformed_json_preserves_context() {
    let err = parse_printer_config("not json").unwrap_err();

    assert!(format!("{err:#}").contains("PANDAR_PRINTERS"));
}

#[test]
fn invalid_printer_config_missing_required_field_preserves_context() {
    let err =
        parse_printer_config(r#"[{"host":"192.0.2.10","serial":"","access_code":"12345678"}]"#)
            .unwrap_err();

    let error = format!("{err:#}");
    assert!(error.contains("PANDAR_PRINTERS"));
    assert!(error.contains("serial"));
}
