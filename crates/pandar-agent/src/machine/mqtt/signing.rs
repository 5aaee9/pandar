use std::{collections::BTreeMap, path::PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use rsa::{
    RsaPrivateKey,
    pkcs1v15::SigningKey,
    pkcs8::DecodePrivateKey,
    signature::{SignatureEncoding, Signer},
};
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};
use sha2::Sha256;

pub(crate) fn maybe_sign_project_file_payload(
    payload: Value,
    printer_model: Option<&str>,
) -> Value {
    let Some(key) = slicer_key() else {
        tracing::warn!(
            "Bambu Studio slicer signing key was not found; sending unsigned project_file"
        );
        return payload;
    };
    let mut project =
        serde_json::from_value::<ProjectFilePayload>(payload.clone()).unwrap_or_default();
    if h2d_family(printer_model) {
        project.flip_nozzle_ids();
    }
    let to_sign = SignedProjectFilePayload {
        print: project.print.clone(),
    };
    let Ok(to_sign_bytes) = serde_json::to_vec(&to_sign) else {
        return payload;
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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct ProjectFilePayload {
    #[serde(default)]
    print: Option<ProjectFilePrint>,
}

#[derive(Debug, Serialize)]
struct SignedProjectFilePayload {
    print: Option<ProjectFilePrint>,
}

#[derive(Debug, Serialize)]
struct SignedProjectFileEnvelope {
    header: SignedProjectFileHeader,
    print: Option<ProjectFilePrint>,
}

#[derive(Debug, Serialize)]
struct SignedProjectFileHeader {
    cert_id: String,
    payload_len: usize,
    sign_alg: &'static str,
    sign_string: String,
    sign_ver: &'static str,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct ProjectFilePrint {
    ams_mapping_info: Option<Vec<AmsMappingInfoEntry>>,
    #[serde(flatten)]
    extra: BTreeMap<String, ProjectFileJson>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(untagged)]
enum ProjectFileJson {
    Object(BTreeMap<String, ProjectFileJson>),
    Array(Vec<ProjectFileJson>),
    String(String),
    Number(Number),
    Bool(bool),
    #[default]
    Null,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct AmsMappingInfoEntry {
    #[serde(default, rename = "nozzleId")]
    nozzle_id: Option<i64>,
    #[serde(flatten)]
    extra: BTreeMap<String, ProjectFileJson>,
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
        let Some(print) = &mut self.print else {
            return;
        };
        let Some(entries) = &mut print.ams_mapping_info else {
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
    use serde::Serialize;
    use serde_json::Value;

    use super::{ProjectFilePayload, h2d_family};

    #[derive(Serialize)]
    struct ProjectFileTestPayload {
        print: ProjectFileTestPrint,
    }

    #[derive(Serialize)]
    struct ProjectFileTestPrint {
        command: &'static str,
        ams_mapping_info: Vec<NozzleMappingEntry>,
    }

    #[derive(Serialize)]
    struct NozzleMappingEntry {
        #[serde(rename = "nozzleId")]
        nozzle_id: u8,
    }

    fn test_payload(nozzle_ids: impl IntoIterator<Item = u8>) -> Value {
        serde_json::to_value(ProjectFileTestPayload {
            print: ProjectFileTestPrint {
                command: "project_file",
                ams_mapping_info: nozzle_ids
                    .into_iter()
                    .map(|nozzle_id| NozzleMappingEntry { nozzle_id })
                    .collect(),
            },
        })
        .expect("test project file payload is serializable")
    }

    fn flip_nozzle_ids(payload: Value) -> Value {
        let mut payload: ProjectFilePayload = serde_json::from_value(payload).unwrap();
        payload.flip_nozzle_ids();
        serde_json::to_value(payload).unwrap()
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

        assert_eq!(payload, test_payload([1, 0, 2]));
    }
}
