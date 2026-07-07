use std::path::PathBuf;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use rsa::{
    RsaPrivateKey,
    pkcs1v15::SigningKey,
    pkcs8::DecodePrivateKey,
    signature::{SignatureEncoding, Signer},
};
use serde_json::{Value, json};
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
    let mut payload = payload;
    if h2d_family(printer_model) {
        flip_nozzle_ids(&mut payload);
    }
    let to_sign = json!({ "print": payload.get("print").cloned().unwrap_or(Value::Null) });
    let Ok(to_sign_bytes) = serde_json::to_vec(&to_sign) else {
        return payload;
    };
    let signing_key = SigningKey::<Sha256>::new(key);
    let signature = signing_key.sign(&to_sign_bytes);
    json!({
        "header": {
            "cert_id": slicer_cert_id().unwrap_or_default(),
            "payload_len": to_sign_bytes.len(),
            "sign_alg": "RSA_SHA256",
            "sign_string": STANDARD.encode(signature.to_bytes()),
            "sign_ver": "v1.0"
        },
        "print": payload.get("print").cloned().unwrap_or(Value::Null)
    })
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

fn flip_nozzle_ids(payload: &mut Value) {
    let Some(entries) = payload
        .pointer_mut("/print/ams_mapping_info")
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for entry in entries {
        let Some(nozzle_id) = entry.get_mut("nozzleId") else {
            continue;
        };
        match nozzle_id.as_i64() {
            Some(0) => *nozzle_id = json!(1),
            Some(1) => *nozzle_id = json!(0),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{flip_nozzle_ids, h2d_family};

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
        let mut payload = json!({
            "print": {
                "command": "project_file",
                "ams_mapping_info": [{"nozzleId": 0}, {"nozzleId": 1}, {"nozzleId": 2}]
            }
        });

        flip_nozzle_ids(&mut payload);

        assert_eq!(
            payload["print"]["ams_mapping_info"],
            json!([{"nozzleId": 1}, {"nozzleId": 0}, {"nozzleId": 2}])
        );
    }
}
