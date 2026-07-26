use serde::Serialize;

pub(super) fn body(path: &str) -> String {
    let Some(url) = reqwest::Url::parse(&format!("http://localhost{path}")).ok() else {
        return pending_body();
    };
    let mut ticket = None;
    let mut redirect_url = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "ticket" => ticket = Some(value.into_owned()),
            "redirect_url" => redirect_url = Some(value.into_owned()),
            _ => {}
        }
    }
    let Some(ticket) = ticket else {
        return pending_body();
    };
    if !valid_ticket(&ticket) || redirect_url.is_none() {
        return pending_body();
    }
    let message = serde_json::to_string(&CallbackMessage {
        command: "user_ticket_login",
        data: CallbackMessageData {
            ticket: ticket.as_str(),
        },
    })
    .expect("callback message is serializable");
    let message = serde_json::to_string(&message).expect("callback script message encodes");
    format!(
        "<!doctype html><html><body><main>Sign-in request received. Return to Studio.</main><script>window.wx?.postMessage?.({message});</script></body></html>"
    )
}

#[derive(Serialize)]
struct CallbackMessage<'a> {
    command: &'static str,
    data: CallbackMessageData<'a>,
}

#[derive(Serialize)]
struct CallbackMessageData<'a> {
    ticket: &'a str,
}

fn valid_ticket(ticket: &str) -> bool {
    !ticket.is_empty()
        && ticket.len() <= 512
        && ticket
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn pending_body() -> String {
    "<!doctype html><html><body><main>Sign-in request received. Return to Studio.</main></body></html>"
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::body;

    #[test]
    fn callback_rejects_script_breakout_ticket() {
        let body = body(
            "/callback?ticket=%3C%2Fscript%3E%3Cscript%3Ealert(1)%3C%2Fscript%3E&redirect_url=x",
        );

        assert!(!body.contains("<script>"));
        assert!(!body.contains("alert(1)"));
    }

    #[test]
    fn callback_delivers_safe_ticket_to_studio() {
        let body = body(
            "/callback?ticket=pandar_plugin_ticket_abc-123&redirect_url=http%3A%2F%2F127.0.0.1%2Fcallback",
        );

        assert!(body.contains("user_ticket_login"));
        assert!(body.contains("pandar_plugin_ticket_abc-123"));
    }
}
