use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct AgentPairingRequest<'a> {
    name: &'a str,
}

pub(super) fn agent_pairing_body(name: &str) -> Option<Value> {
    Some(serde_json::to_value(AgentPairingRequest { name }).unwrap())
}
