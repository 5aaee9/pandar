use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct AgentCreateRequest<'a> {
    name: &'a str,
}

pub(super) fn agent_name_body(name: &str) -> Option<Value> {
    Some(serde_json::to_value(AgentCreateRequest { name }).unwrap())
}
