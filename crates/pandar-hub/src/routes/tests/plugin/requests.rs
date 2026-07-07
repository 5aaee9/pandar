use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct PluginLoginTicketRequest<'a> {
    redirect_url: &'a str,
}

#[derive(Serialize)]
struct PluginTicketExchangeRequest<'a> {
    ticket: &'a str,
}

#[derive(Serialize)]
struct RedactedAuditMetadataFixture {
    safe: &'static str,
    subject: &'static str,
    plaintext_token: &'static str,
    ticket: &'static str,
    plaintext_ticket: &'static str,
    nested: RedactedNestedAuditMetadataFixture,
    headers: RedactedHeadersFixture,
    artifact_storage_path: &'static str,
}

#[derive(Serialize)]
struct RedactedNestedAuditMetadataFixture {
    credential_hash: &'static str,
    provider_subject: &'static str,
    ticket_hash: &'static str,
    token_hash: &'static str,
    ok: bool,
}

#[derive(Serialize)]
struct RedactedHeadersFixture {
    #[serde(rename = "Authorization")]
    authorization: &'static str,
}

#[derive(Serialize)]
struct SafeAuditMetadataFixture<'a> {
    safe: &'a str,
}

pub(super) fn plugin_login_ticket_body(redirect_url: &str) -> Option<Value> {
    Some(value(PluginLoginTicketRequest { redirect_url }))
}

pub(super) fn plugin_ticket_exchange_body(ticket: &str) -> Option<Value> {
    Some(value(PluginTicketExchangeRequest { ticket }))
}

pub(super) fn redacted_audit_metadata_fixture() -> Value {
    value(RedactedAuditMetadataFixture {
        safe: "keep",
        subject: "external-subject",
        plaintext_token: "secret",
        ticket: "ticket",
        plaintext_ticket: "ticket",
        nested: RedactedNestedAuditMetadataFixture {
            credential_hash: "hash",
            provider_subject: "external-subject",
            ticket_hash: "hash",
            token_hash: "hash",
            ok: true,
        },
        headers: RedactedHeadersFixture {
            authorization: "Bearer secret",
        },
        artifact_storage_path: "/tmp/secret",
    })
}

pub(super) fn safe_audit_metadata_fixture(safe: &str) -> Value {
    value(SafeAuditMetadataFixture { safe })
}

fn value(input: impl Serialize) -> Value {
    serde_json::to_value(input).unwrap()
}
