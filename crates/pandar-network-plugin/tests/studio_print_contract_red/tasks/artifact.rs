use std::{env, fs, path::PathBuf};

use crate::harness::{PLUGIN_OVERRIDE_ENV, ProbeEvidence};
use sha2::{Digest, Sha256};

pub(super) fn assert_plugin_identity_reported(evidence: &ProbeEvidence) {
    let reported = PathBuf::from(&evidence.plugin_path);
    assert_eq!(reported, fs::canonicalize(&reported).unwrap());
    if let Some(configured) = env::var_os(PLUGIN_OVERRIDE_ENV) {
        assert_eq!(reported, fs::canonicalize(configured).unwrap());
        assert_eq!(evidence.plugin_source, "override");
    } else {
        assert_eq!(evidence.plugin_source, "debug-build");
    }
    assert_eq!(
        evidence.plugin_sha256,
        format!("{:x}", Sha256::digest(fs::read(reported).unwrap()))
    );
}
