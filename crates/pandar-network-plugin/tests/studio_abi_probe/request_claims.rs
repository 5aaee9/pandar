use super::{MockMode, ProbeOutput, assert_json_field, run_probe};

#[path = "firmware_snapshot_claim.rs"]
mod firmware_snapshot_claim;

#[test]
fn compiled_probe_rejects_unauthorized_and_transition_requests_before_hub_io() {
    let ProbeOutput { stdout, stderr, .. } =
        run_probe(MockMode::RequestAdmission, "request-admission");

    assert!(
        stderr.is_empty(),
        "request admission probe stderr was not empty: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
}

#[test]
fn probe_firmware_callback_generation_and_copy_share_final_claim() {
    let ProbeOutput { stdout, stderr, .. } =
        run_probe(MockMode::FirmwareClaimRace, "firmware-claim-race");

    assert!(
        stderr.is_empty(),
        "firmware final claim probe stderr was not empty: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
}
