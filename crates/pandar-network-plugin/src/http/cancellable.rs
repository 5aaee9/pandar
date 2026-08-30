use std::io::Write;

use anyhow::Context;
use serde::Serialize;

use crate::{
    NO_AUTH_CONNECT_FAILURE_STATUS, PluginHttpResult, RequestKind,
    cancellation::RequestCancellation, network_error, result, runtime, stable_error_body,
};

use super::{
    NO_AUTH_SESSION_POST_TIMEOUT, PLUGIN_SESSION_DELETE_TIMEOUT, execute_request, hub_client,
    post_request_context, response_result_with_writer, write_network_error,
};

#[cfg(test)]
mod tests;

pub(crate) fn post_json_with_connect_failure(
    url: &str,
    body: impl Serialize,
    kind: RequestKind,
    cancellation: RequestCancellation,
) -> PluginHttpResult {
    super::diagnostics::buffered(|writer| {
        post_json_with_connect_failure_with_writer(url, body, kind, cancellation, writer)
    })
}

pub(super) fn post_json_with_connect_failure_with_writer(
    url: &str,
    body: impl Serialize,
    kind: RequestKind,
    cancellation: RequestCancellation,
    writer: &mut impl Write,
) -> PluginHttpResult {
    let response = runtime().block_on(async {
        let request = hub_client().post(url).json(&body);
        tokio::select! {
            biased;
            () = cancellation.wait() => None,
            response = execute_request(request, Some(NO_AUTH_SESSION_POST_TIMEOUT)) => {
                Some(response.context(post_request_context(kind)))
            }
        }
    });
    match response {
        None => cancelled(),
        Some(Ok(response)) => response_result_with_writer(response, kind, writer),
        Some(Err(error)) => {
            write_network_error(writer, &error);
            if error
                .downcast_ref::<reqwest::Error>()
                .is_some_and(reqwest::Error::is_connect)
            {
                result(
                    NO_AUTH_CONNECT_FAILURE_STATUS,
                    0,
                    stable_error_body("hub_unavailable"),
                )
            } else {
                network_error()
            }
        }
    }
}

pub(crate) fn delete_session(
    url: &str,
    token: &str,
    kind: RequestKind,
    cancellation: RequestCancellation,
) -> PluginHttpResult {
    super::diagnostics::buffered(|writer| {
        delete_session_with_writer(url, token, kind, cancellation, writer)
    })
}

fn delete_session_with_writer(
    url: &str,
    token: &str,
    kind: RequestKind,
    cancellation: RequestCancellation,
    writer: &mut impl Write,
) -> PluginHttpResult {
    let response = runtime().block_on(async {
        let request = hub_client().delete(url).bearer_auth(token);
        tokio::select! {
            biased;
            () = cancellation.wait() => None,
            response = execute_request(request, Some(PLUGIN_SESSION_DELETE_TIMEOUT)) => {
                Some(response.context("DELETE plugin session request"))
            }
        }
    });
    match response {
        None => cancelled(),
        Some(Ok(response)) => response_result_with_writer(response, kind, writer),
        Some(Err(error)) => {
            write_network_error(writer, &error);
            network_error()
        }
    }
}

fn cancelled() -> PluginHttpResult {
    result(1, 0, stable_error_body("request_cancelled"))
}
