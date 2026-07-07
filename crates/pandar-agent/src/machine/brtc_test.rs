use serde_json::json;

use super::*;

#[test]
fn md5_helpers_match_bambu_case_usage() {
    assert_eq!(md5_lower(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(md5_upper(b"abc"), "900150983CD24FB0D6963F7D28E17F72");
}

#[test]
fn json_prefix_stops_before_binary_separator() {
    let body =
        br#"{"result":0}"#.iter().copied().chain(b"\n\nabc".iter().copied()).collect::<Vec<_>>();
    assert_eq!(json_prefix_len(&body), Some(12));
}

#[test]
fn upload_chunk_request_only_includes_md5_for_final_chunk() {
    assert_eq!(
        serde_json::to_value(protocol::upload_chunk_request(7, 0, 0, 1024, None)).unwrap(),
        json!({
            "cmdtype": BRTC_FILE_UPLOAD_CMD,
            "sequence": 7,
            "req": {
                "frag_id": 0,
                "offset": 0,
                "size": 1024
            }
        })
    );
    assert_eq!(
        serde_json::to_value(protocol::upload_chunk_request(
            7,
            1,
            1024,
            512,
            Some("abc123")
        ))
        .unwrap(),
        json!({
            "cmdtype": BRTC_FILE_UPLOAD_CMD,
            "sequence": 7,
            "req": {
                "frag_id": 1,
                "offset": 1024,
                "size": 512,
                "file_md5": "abc123"
            }
        })
    );
}
