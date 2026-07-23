use super::*;

fn transfer(model: &str) -> BambuMachineFileTransfer {
    BambuMachineFileTransfer::new(BambuPrinterEndpoint {
        host: "192.0.2.10".to_owned(),
        serial: "01P00A000000001".to_owned(),
        access_code: "12345678".to_owned(),
        model: Some(model.to_owned()),
        name: None,
    })
}

#[test]
fn print_policy_only_disables_brtc_for_the_current_print_upload() {
    let x1_transfer = transfer("X1 Carbon");
    let enabled = PrintUploadPolicy {
        try_emmc_print: true,
    };
    let disabled = PrintUploadPolicy {
        try_emmc_print: false,
    };

    assert!(x1_transfer.should_try_brtc_upload("job.gcode.3mf", enabled));
    assert!(!x1_transfer.should_try_brtc_upload("job.gcode.3mf", disabled));
    assert!(!x1_transfer.should_try_brtc_upload("job.3mf", enabled));
    assert!(!transfer("A1").should_try_brtc_upload("job.gcode.3mf", enabled));
}

#[test]
fn generic_upload_retains_prior_brtc_eligibility() {
    assert!(transfer("X1 Carbon").should_try_brtc_upload("job.gcode.3mf", GENERIC_UPLOAD_POLICY));
}
