use anyhow::Context;
use futures_util::TryStreamExt;

use super::PLUGIN_HTTP_MAX_RESPONSE_BYTES;

pub(crate) async fn read_bounded_response_bytes(
    response: reqwest::Response,
    max_bytes: usize,
    overflow_error: &str,
) -> anyhow::Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        anyhow::bail!(overflow_error.to_owned());
    }
    let mut bytes = Vec::with_capacity(
        response
            .content_length()
            .unwrap_or_default()
            .min(max_bytes as u64) as usize,
    );
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream
        .try_next()
        .await
        .map_err(reqwest::Error::without_url)?
    {
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            anyhow::bail!(overflow_error.to_owned());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(crate) async fn read_bounded_response_body(
    response: reqwest::Response,
) -> anyhow::Result<String> {
    read_bounded_response_body_with_limit(response, PLUGIN_HTTP_MAX_RESPONSE_BYTES).await
}

pub(crate) async fn read_bounded_response_body_with_limit(
    response: reqwest::Response,
    max_bytes: usize,
) -> anyhow::Result<String> {
    let overflow_error = format!("Hub HTTP response exceeds {max_bytes} bytes");
    let bytes = read_bounded_response_bytes(response, max_bytes, &overflow_error).await?;
    String::from_utf8(bytes).context("Hub HTTP response is not UTF-8")
}
