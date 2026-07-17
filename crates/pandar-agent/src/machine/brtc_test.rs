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

#[test]
fn frame_payload_length_rejects_values_above_limit() {
    assert_eq!(
        checked_frame_payload_len(BRTC_MAX_FRAME_PAYLOAD_SIZE as u32).unwrap(),
        BRTC_MAX_FRAME_PAYLOAD_SIZE
    );
    let (logs, error) = crate::test_tracing::capture_logs(|| {
        checked_frame_payload_len(BRTC_MAX_FRAME_PAYLOAD_SIZE as u32 + 1).unwrap_err()
    });
    assert!(format!("{error:#}").contains("exceeds limit"));
    let captured = logs.contents();
    assert!(captured.contains("rejecting oversized BRTC frame payload"));
    assert!(captured.contains("payload_len=16777217"));
    assert!(captured.contains("limit=16777216"));
}

#[test]
fn upload_reply_rejects_overflowing_chunk_size() {
    let reply = serde_json::from_value(serde_json::json!({
        "cmdtype": BRTC_FILE_UPLOAD_CMD,
        "sequence": 7,
        "result": 1,
        "reply": {"chunk_size": u64::MAX, "offset": 0}
    }))
    .unwrap();
    let frame = protocol::upload_reply("reply".to_owned(), reply, 7).unwrap();

    let error = frame.chunk_size_bytes().unwrap_err();
    assert!(format!("{error:#}").contains("chunk_size"));
}

#[test]
fn upload_reply_rejects_chunk_size_above_limit() {
    let chunk_size_kib = BRTC_MAX_UPLOAD_CHUNK_SIZE as u64 / 1024 + 1;
    let reply = serde_json::from_value(serde_json::json!({
        "cmdtype": BRTC_FILE_UPLOAD_CMD,
        "sequence": 7,
        "result": 1,
        "reply": {"chunk_size": chunk_size_kib, "offset": 0}
    }))
    .unwrap();
    let frame = protocol::upload_reply("reply".to_owned(), reply, 7).unwrap();

    let (logs, error) = crate::test_tracing::capture_logs(|| frame.chunk_size_bytes().unwrap_err());
    assert!(format!("{error:#}").contains("exceeds limit"));
    let captured = logs.contents();
    assert!(captured.contains("rejecting oversized BRTC upload chunk size"));
    assert!(captured.contains("chunk_size=16778240"));
    assert!(captured.contains("limit=16777216"));
}

#[test]
fn chunk_end_rejects_integer_overflow() {
    let error = checked_chunk_end(usize::MAX, 1, usize::MAX).unwrap_err();
    assert!(format!("{error:#}").contains("offset"));
}

#[test]
fn binary_frame_payload_uses_checked_length_and_delimiter() {
    assert_eq!(
        append_binary_frame_payload(b"{}".to_vec(), b"abc").unwrap(),
        b"{}\n\nabc"
    );

    let error = checked_binary_frame_payload_len(usize::MAX, 1).unwrap_err();
    assert!(format!("{error:#}").contains("overflowed"));
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
