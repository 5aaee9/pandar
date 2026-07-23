use crate::{
    NO_AUTH_CONNECT_FAILURE_STATUS, PluginHttpResult, RequestKind,
    cancellation::RequestCancellation,
    http::{self, EmptyRequest, TicketExchangeRequest},
    invalid_input, normalize_hub_url, read_utf8,
};

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_exchange_ticket(
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    ticket_ptr: *const u8,
    ticket_len: usize,
) -> PluginHttpResult {
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len).and_then(normalize_hub_url) else {
        return invalid_input("invalid_hub_url");
    };
    let Some(ticket) = read_utf8(ticket_ptr, ticket_len).filter(|ticket| !ticket.trim().is_empty())
    else {
        return invalid_input("invalid_plugin_ticket");
    };
    http::post_json(
        &format!("{hub_url}/api/v1/plugin/login-tickets/exchange"),
        None,
        TicketExchangeRequest { ticket: &ticket },
        RequestKind::TicketExchange,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_create_no_auth_session(
    hub_url_ptr: *const u8,
    hub_url_len: usize,
) -> PluginHttpResult {
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len).and_then(normalize_hub_url) else {
        return invalid_input("invalid_hub_url");
    };
    create_no_auth_session_with_cancellation(&hub_url, RequestCancellation::disabled())
}

pub(crate) fn create_no_auth_session_with_cancellation(
    hub_url: &str,
    cancellation: RequestCancellation,
) -> PluginHttpResult {
    http::cancellable::post_json_with_connect_failure(
        &format!("{hub_url}/api/v1/plugin/no-auth-session"),
        EmptyRequest {},
        RequestKind::TicketExchange,
        cancellation,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_no_auth_retryable_connect_failure(status: i32) -> bool {
    status == NO_AUTH_CONNECT_FAILURE_STATUS
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_delete_session(
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    token_ptr: *const u8,
    token_len: usize,
) -> PluginHttpResult {
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len).and_then(normalize_hub_url) else {
        return invalid_input("invalid_hub_url");
    };
    let Some(token) = read_utf8(token_ptr, token_len).filter(|token| !token.trim().is_empty())
    else {
        return invalid_input("invalid_auth_token");
    };

    http::delete_session(
        &format!("{hub_url}/api/v1/plugin/session"),
        &token,
        RequestKind::PluginSession,
    )
}
