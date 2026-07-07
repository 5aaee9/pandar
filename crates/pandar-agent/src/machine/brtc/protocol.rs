use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{BRTC_CTRL_JSON_MTYPE, BRTC_CTRL_SETUP_MTYPE, BRTC_FILE_UPLOAD_CMD};

#[derive(Debug, Serialize)]
struct BrtcSetupRequest<'a> {
    sequence: u32,
    mtype: i64,
    req: BrtcSetupRequestBody<'a>,
}

#[derive(Debug, Serialize)]
struct BrtcSetupRequestBody<'a> {
    t_av: u8,
    mtype: i64,
    peer_t: u8,
    pid: String,
    ver: &'a str,
}

#[derive(Debug, Serialize)]
struct BrtcUploadInitRequest<'a> {
    cmdtype: i64,
    sequence: u32,
    req: BrtcUploadInitBody<'a>,
}

#[derive(Debug, Serialize)]
struct BrtcUploadInitBody<'a> {
    #[serde(rename = "type")]
    upload_type: &'a str,
    path: &'a str,
    total: usize,
    storage: &'a str,
}

#[derive(Debug, Serialize)]
struct BrtcUploadChunkRequest<'a> {
    cmdtype: i64,
    sequence: u32,
    req: BrtcUploadChunkBody<'a>,
}

#[derive(Debug, Serialize)]
struct BrtcUploadChunkBody<'a> {
    frag_id: u32,
    offset: usize,
    size: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_md5: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct BrtcCtrlJson<'a, T: ?Sized> {
    mtype: i64,
    #[serde(flatten)]
    payload: &'a T,
}

#[derive(Debug, Deserialize)]
struct BrtcSetupAck {
    mtype: Option<i64>,
    result: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct BrtcUploadReply {
    cmdtype: Option<i64>,
    sequence: Option<u32>,
    result: Option<i64>,
    reply: Option<BrtcUploadReplyBody>,
}

#[derive(Debug, Deserialize)]
struct BrtcUploadReplyBody {
    chunk_size: Option<u64>,
    offset: Option<u64>,
}

#[derive(Debug)]
pub(super) struct BrtcUploadReplyFrame {
    raw: Value,
    reply: BrtcUploadReply,
}

pub(super) fn setup_request(serial: &str) -> impl Serialize {
    BrtcSetupRequest {
        sequence: 0,
        mtype: BRTC_CTRL_SETUP_MTYPE,
        req: BrtcSetupRequestBody {
            t_av: 1,
            mtype: BRTC_CTRL_JSON_MTYPE,
            peer_t: 3,
            pid: format!("pandar-{serial}"),
            ver: "02.08.00.53",
        },
    }
}

pub(super) fn setup_ack_success(value: Value) -> bool {
    serde_json::from_value::<BrtcSetupAck>(value)
        .is_ok_and(|ack| ack.mtype == Some(BRTC_CTRL_SETUP_MTYPE) && ack.result == Some(0))
}

pub(super) fn upload_init_request(
    sequence: u32,
    dest_name: &str,
    total: usize,
) -> impl Serialize + '_ {
    BrtcUploadInitRequest {
        cmdtype: BRTC_FILE_UPLOAD_CMD,
        sequence,
        req: BrtcUploadInitBody {
            upload_type: "model",
            path: dest_name,
            total,
            storage: "emmc",
        },
    }
}

pub(super) fn upload_chunk_request(
    sequence: u32,
    fragment: u32,
    offset: usize,
    size: usize,
    file_md5: Option<&str>,
) -> impl Serialize + '_ {
    BrtcUploadChunkRequest {
        cmdtype: BRTC_FILE_UPLOAD_CMD,
        sequence,
        req: BrtcUploadChunkBody {
            frag_id: fragment,
            offset,
            size,
            file_md5,
        },
    }
}

pub(super) fn upload_reply(value: Value, sequence: u32) -> Option<BrtcUploadReplyFrame> {
    let reply = serde_json::from_value::<BrtcUploadReply>(value.clone()).ok()?;
    if reply.cmdtype == Some(BRTC_FILE_UPLOAD_CMD) && reply.sequence == Some(sequence) {
        Some(BrtcUploadReplyFrame { raw: value, reply })
    } else {
        None
    }
}

pub(super) fn wrap_ctrl_json<T: Serialize + ?Sized>(value: &T) -> anyhow::Result<String> {
    serde_json::to_string(&BrtcCtrlJson {
        mtype: BRTC_CTRL_JSON_MTYPE,
        payload: value,
    })
    .context("encode BRTC ABI payload")
}

impl BrtcUploadReplyFrame {
    pub(super) fn result(&self) -> i64 {
        self.reply.result.unwrap_or(-1)
    }

    pub(super) fn raw(&self) -> &Value {
        &self.raw
    }

    pub(super) fn chunk_size_bytes(&self) -> anyhow::Result<usize> {
        self.reply
            .reply
            .as_ref()
            .and_then(|reply| reply.chunk_size)
            .filter(|value| *value > 0)
            .map(|value| value as usize * 1024)
            .ok_or_else(|| anyhow!("BRTC upload init reply did not include chunk_size"))
    }

    pub(super) fn offset(&self) -> usize {
        self.reply
            .reply
            .as_ref()
            .and_then(|reply| reply.offset)
            .unwrap_or(0) as usize
    }
}
