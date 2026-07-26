use anyhow::Context;
use futures_util::TryStreamExt;

use super::PLUGIN_HTTP_MAX_RESPONSE_BYTES;

pub(crate) async fn read_bounded_response_body(
    response: reqwest::Response,
) -> anyhow::Result<String> {
    if response
        .content_length()
        .is_some_and(|length| length > PLUGIN_HTTP_MAX_RESPONSE_BYTES as u64)
    {
        anyhow::bail!("plugin HTTP response exceeds {PLUGIN_HTTP_MAX_RESPONSE_BYTES} bytes");
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream
        .try_next()
        .await
        .map_err(reqwest::Error::without_url)?
    {
        if bytes.len().saturating_add(chunk.len()) > PLUGIN_HTTP_MAX_RESPONSE_BYTES {
            anyhow::bail!("plugin HTTP response exceeds {PLUGIN_HTTP_MAX_RESPONSE_BYTES} bytes");
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).context("plugin HTTP response is not UTF-8")
}
