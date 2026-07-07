use serde::Serialize;

use super::*;

#[derive(Serialize)]
struct ExpectedUploadChunkRequest<'a> {
    cmdtype: i64,
    sequence: u32,
    req: ExpectedUploadChunkBody<'a>,
}

#[derive(Serialize)]
struct ExpectedUploadChunkBody<'a> {
    frag_id: u32,
    offset: usize,
    size: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_md5: Option<&'a str>,
}

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
        expected_upload_chunk_request(7, 0, 0, 1024, None)
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
        expected_upload_chunk_request(7, 1, 1024, 512, Some("abc123"))
    );
}

fn expected_upload_chunk_request(
    sequence: u32,
    fragment: u32,
    offset: usize,
    size: usize,
    file_md5: Option<&str>,
) -> serde_json::Value {
    serde_json::to_value(ExpectedUploadChunkRequest {
        cmdtype: BRTC_FILE_UPLOAD_CMD,
        sequence,
        req: ExpectedUploadChunkBody {
            frag_id: fragment,
            offset,
            size,
            file_md5,
        },
    })
    .unwrap()
}
