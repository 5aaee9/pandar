use jsonwebtoken::{Algorithm, EncodingKey, Header, encode, jwk::JwkSet};
use serde::Serialize;

use super::*;

pub(super) fn external_auth_state(state: AppState) -> AppState {
    let config = crate::identity::ExternalAuthConfig {
        provider: TEST_PROVIDER.to_owned(),
        issuer: TEST_ISSUER.to_owned(),
        jwks_url: "https://identity.example.test/.well-known/jwks.json".to_owned(),
        audience: Some(TEST_AUDIENCE.to_owned()),
        algorithms: vec![Algorithm::RS256],
        authorized_parties: Vec::new(),
        required_scopes: Vec::new(),
        leeway_seconds: 60,
    };
    let jwks = serde_json::from_str::<JwkSet>(TEST_PUBLIC_JWK_JSON).unwrap();
    state.with_external_auth(crate::identity::JwtVerifier::static_jwks(config, jwks))
}

pub(super) fn jwt_for(
    subject: &str,
    issuer: &str,
    audience: &str,
    kid: &str,
    exp_offset_seconds: i64,
) -> String {
    jwt_for_claims(ExternalAuthClaims {
        kid,
        iss: issuer,
        sub: subject,
        aud: audience,
        exp_offset_seconds,
        email: None,
        email_verified: None,
        name: None,
        preferred_username: None,
    })
}

pub(super) fn jwt_for_profile(
    subject: &str,
    email: &str,
    email_verified: bool,
    name: &str,
) -> String {
    jwt_for_claims(ExternalAuthClaims {
        kid: "test-key",
        iss: TEST_ISSUER,
        sub: subject,
        aud: TEST_AUDIENCE,
        exp_offset_seconds: 3600,
        email: Some(email),
        email_verified: Some(email_verified),
        name: Some(name),
        preferred_username: None,
    })
}

fn jwt_for_claims(claims: ExternalAuthClaims<'_>) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(claims.kid.to_owned());
    let now = jsonwebtoken::get_current_timestamp() as i64;
    let exp = now.saturating_add(claims.exp_offset_seconds).max(0) as u64;
    let nbf = now.saturating_sub(30).max(0) as u64;
    encode(
        &header,
        &EncodedExternalAuthClaims {
            iss: claims.iss,
            sub: claims.sub,
            aud: claims.aud,
            exp,
            nbf,
            email: claims.email,
            email_verified: claims.email_verified,
            name: claims.name,
            preferred_username: claims.preferred_username,
        },
        &EncodingKey::from_rsa_pem(TEST_PRIVATE_KEY_PEM.as_bytes()).unwrap(),
    )
    .unwrap()
}

pub(super) async fn external_auth_token_for_role(
    state: &AppState,
    tenant_id: TenantId,
    role: crate::repositories::UserRole,
    subject: &str,
) -> String {
    let user = state
        .auth()
        .create_user(
            tenant_id,
            format!("{subject}@example.test"),
            "External Test User",
            role,
        )
        .await
        .unwrap();
    state
        .auth()
        .link_external_identity(tenant_id, &user.id, TEST_PROVIDER, subject)
        .await
        .unwrap();
    jwt_for_profile(
        subject,
        &format!("{subject}@example.test"),
        true,
        "External Test User",
    )
}

#[derive(Serialize)]
struct ExternalAuthClaims<'a> {
    kid: &'a str,
    iss: &'a str,
    sub: &'a str,
    aud: &'a str,
    exp_offset_seconds: i64,
    email: Option<&'a str>,
    email_verified: Option<bool>,
    name: Option<&'a str>,
    preferred_username: Option<&'a str>,
}

#[derive(Serialize)]
struct EncodedExternalAuthClaims<'a> {
    iss: &'a str,
    sub: &'a str,
    aud: &'a str,
    exp: u64,
    nbf: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preferred_username: Option<&'a str>,
}
