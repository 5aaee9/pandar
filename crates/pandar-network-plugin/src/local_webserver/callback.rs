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
    if redirect_url.is_none() {
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

fn pending_body() -> String {
    "<!doctype html><html><body><main>Sign-in request received. Return to Studio.</main></body></html>"
        .to_owned()
}
