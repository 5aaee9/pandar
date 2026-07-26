use std::path::PathBuf;

use aws_lc_rs::{
    rand::SystemRandom,
    signature::{RSA_PKCS1_SHA256, RsaKeyPair},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rustls::pki_types::{PrivateKeyDer, pem::PemObject};
use serde::Serialize;
use serde_json::Value;

use super::commands::payload::{ProjectFilePayload, ProjectFilePayloadPrint, json_payload};

pub(crate) fn maybe_sign_project_file_payload(
    mut project: ProjectFilePayload,
    printer_model: Option<&str>,
) -> Value {
    let Some(key) = slicer_key() else {
        tracing::warn!(
            "Bambu Studio slicer signing key was not found; sending unsigned project_file"
        );
        return json_payload(project);
    };
    if h2d_family(printer_model) {
        project.flip_nozzle_ids();
    }
    let to_sign = SignedProjectFilePayload {
        print: project.print.clone(),
    };
    let Ok(to_sign_bytes) = serde_json::to_vec(&to_sign) else {
        return json_payload(project);
    };
    let mut signature = vec![0_u8; key.public_modulus_len()];
    if key
        .sign(
            &RSA_PKCS1_SHA256,
            &SystemRandom::new(),
            &to_sign_bytes,
            &mut signature,
        )
        .is_err()
    {
        tracing::warn!("failed to sign Bambu project_file payload");
        return json_payload(project);
    }
    serde_json::to_value(SignedProjectFileEnvelope {
        header: SignedProjectFileHeader {
            cert_id: slicer_cert_id().unwrap_or_default(),
            payload_len: to_sign_bytes.len(),
            sign_alg: "RSA_SHA256",
            sign_string: STANDARD.encode(signature),
            sign_ver: "v1.0",
        },
        print: project.print,
    })
    .expect("signed project file payload is serializable")
}

#[derive(Serialize)]
struct SignedProjectFilePayload {
    print: ProjectFilePayloadPrint,
}

#[derive(Serialize)]
struct SignedProjectFileEnvelope {
    header: SignedProjectFileHeader,
    print: ProjectFilePayloadPrint,
}

#[derive(Debug, Serialize)]
struct SignedProjectFileHeader {
    cert_id: String,
    payload_len: usize,
    sign_alg: &'static str,
    sign_string: String,
    sign_ver: &'static str,
}

fn slicer_key() -> Option<RsaKeyPair> {
    let path = slicer_key_path()?;
    let pem = std::fs::read(path).ok()?;
    let key = PrivateKeyDer::from_pem_slice(&pem).ok()?;
    RsaKeyPair::from_pkcs8(key.secret_der())
        .or_else(|_| RsaKeyPair::from_der(key.secret_der()))
        .ok()
}

fn slicer_cert_id() -> Option<String> {
    if let Ok(value) = std::env::var("BBL_SLICER_CERT_ID")
        && !value.trim().is_empty()
    {
        return Some(value);
    }
    let mut path = slicer_key_path()?;
    path.set_file_name("slicer_cert_id.txt");
    std::fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn slicer_key_path() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("BBL_SLICER_KEY_PEM")
        && !value.trim().is_empty()
    {
        return Some(PathBuf::from(value));
    }
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(appdata)
            .join("BambuStudio")
            .join("slicer_key.pem"),
    )
}

fn h2d_family(model: Option<&str>) -> bool {
    let Some(model) = model else {
        return false;
    };
    let model = model.to_ascii_uppercase();
    ["H2D", "O1D", "C16", "X2D"]
        .iter()
        .any(|token| model.contains(token))
}

impl ProjectFilePayload {
    fn flip_nozzle_ids(&mut self) {
        let Some(entries) = &mut self.print.ams_mapping_info else {
            return;
        };
        for entry in entries {
            match entry.nozzle_id {
                Some(0) => entry.nozzle_id = Some(1),
                Some(1) => entry.nozzle_id = Some(0),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use crate::machine::mqtt::commands::{
        ProjectFileAmsMappingInfo, payload::ProjectFilePayloadPrint,
    };

    use super::{ProjectFilePayload, h2d_family, json_payload};

    fn test_payload(nozzle_ids: impl IntoIterator<Item = i32>) -> ProjectFilePayload {
        ProjectFilePayload {
            print: ProjectFilePayloadPrint {
                command: "project_file",
                sequence_id: "20000".to_owned(),
                param: "Metadata/plate_1.gcode".to_owned(),
                project_id: "0".to_owned(),
                profile_id: "0".to_owned(),
                task_id: "0".to_owned(),
                subtask_id: "0".to_owned(),
                subtask_name: "job".to_owned(),
                url: "ftp://job.3mf".to_owned(),
                file: "job.3mf".to_owned(),
                md5: String::new(),
                bed_type: "auto".to_owned(),
                bed_leveling: false,
                flow_cali: false,
                vibration_cali: false,
                layer_inspect: false,
                timelapse: false,
                use_ams: true,
                ams_mapping: Vec::new(),
                ams_mapping2: Vec::new(),
                nozzle_mapping: None,
                ams_mapping_info: Some(
                    nozzle_ids
                        .into_iter()
                        .map(|nozzle_id| ProjectFileAmsMappingInfo {
                            ams: 0,
                            target_color: String::new(),
                            filament_id: String::new(),
                            filament_type: String::new(),
                            nozzle_id: Some(nozzle_id),
                            source_color: None,
                        })
                        .collect(),
                ),
                auto_bed_leveling: 0,
                nozzle_offset_cali: 0,
                cfg: "0".to_owned(),
                extrude_cali_flag: 0,
                extrude_cali_manual_mode: Some(-1),
            },
        }
    }

    fn flip_nozzle_ids(mut payload: ProjectFilePayload) -> Value {
        payload.flip_nozzle_ids();
        json_payload(payload)
    }

    #[test]
    fn h2d_family_matches_new_dual_nozzle_models() {
        assert!(h2d_family(Some("Bambu Lab X2D")));
        assert!(h2d_family(Some("3DPrinter-O1D-xxx")));
        assert!(h2d_family(Some("printer-c16-v2")));
        assert!(!h2d_family(Some("P2S")));
        assert!(!h2d_family(None));
    }

    #[test]
    fn flip_nozzle_ids_only_swaps_zero_and_one() {
        let payload = flip_nozzle_ids(test_payload([0, 1, 2]));

        assert_eq!(payload, json_payload(test_payload([1, 0, 2])));
    }
}
