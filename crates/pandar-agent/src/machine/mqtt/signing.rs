use std::path::PathBuf;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use rsa::{
    RsaPrivateKey,
    pkcs1v15::SigningKey,
    pkcs8::DecodePrivateKey,
    signature::{SignatureEncoding, Signer},
};
use serde::Serialize;
use serde_json::Value;
use sha2::Sha256;

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
    let signing_key = SigningKey::<Sha256>::new(key);
    let signature = signing_key.sign(&to_sign_bytes);
    serde_json::to_value(SignedProjectFileEnvelope {
        header: SignedProjectFileHeader {
            cert_id: slicer_cert_id().unwrap_or_default(),
            payload_len: to_sign_bytes.len(),
            sign_alg: "RSA_SHA256",
            sign_string: STANDARD.encode(signature.to_bytes()),
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

fn slicer_key() -> Option<RsaPrivateKey> {
    let path = slicer_key_path()?;
    let pem = std::fs::read_to_string(path).ok()?;
    RsaPrivateKey::from_pkcs8_pem(&pem).ok()
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
                0 => entry.nozzle_id = 1,
                1 => entry.nozzle_id = 0,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use crate::machine::mqtt::commands::payload::{
        ProjectFileAmsMappingInfo, ProjectFilePayloadPrint,
    };

    use super::{ProjectFilePayload, h2d_family, json_payload};

    fn test_payload(nozzle_ids: impl IntoIterator<Item = i64>) -> ProjectFilePayload {
        ProjectFilePayload {
            print: ProjectFilePayloadPrint {
                command: "project_file",
                sequence_id: "20000".to_owned(),
                param: "Metadata/plate_1.gcode".to_owned(),
                project_id: "0",
                profile_id: "0",
                task_id: "0",
                subtask_id: "0",
                subtask_name: "job".to_owned(),
                url: "ftp://job.3mf".to_owned(),
                file: "job.3mf".to_owned(),
                md5: String::new(),
                bed_type: "auto",
                bed_leveling: false,
                flow_cali: false,
                vibration_cali: false,
                layer_inspect: false,
                timelapse: false,
                use_ams: true,
                ams_mapping: Vec::new(),
                ams_mapping2: Vec::new(),
                ams_mapping_info: Some(
                    nozzle_ids
                        .into_iter()
                        .map(|nozzle_id| ProjectFileAmsMappingInfo {
                            nozzle_id,
                            extra: Default::default(),
                        })
                        .collect(),
                ),
                auto_bed_leveling: 0,
                nozzle_offset_cali: 0,
                cfg: "0",
                extrude_cali_flag: 0,
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
