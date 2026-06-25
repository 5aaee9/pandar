use jsonwebtoken::{Algorithm, EncodingKey, Header, encode, jwk::JwkSet};
use serde::{Deserialize, Serialize};

use super::{
    config::tests_support::{AUDIENCE_VAR, REQUIRED_SCOPES_VAR, config_from_vars},
    verifier::{AudienceClaim, JwtVerifier},
};

const TEST_PRIVATE_KEY: &str = r#"-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAyRE6rHuNR0QbHO3H3Kt2pOKGVhQqGZXInOduQNxXzuKlvQTL
UTv4l4sggh5/CYYi/cvI+SXVT9kPWSKXxJXBXd/4LkvcPuUakBoAkfh+eiFVMh2V
rUyWyj3MFl0HTVF9KwRXLAcwkREiS3npThHRyIxuy0ZMeZfxVL5arMhw1SRELB8H
oGfG/AtH89BIE9jDBHZ9dLelK9a184zAf8LwoPLxvJb3Il5nncqPcSfKDDodMFBI
Mc4lQzDKL5gvmiXLXB1AGLm8KBjfE8s3L5xqi+yUod+j8MtvIj812dkS4QMiRVN/
by2h3ZY8LYVGrqZXZTcgn2ujn8uKjXLZVD5TdQIDAQABAoIBAHREk0I0O9DvECKd
WUpAmF3mY7oY9PNQiu44Yaf+AoSuyRpRUGTMIgc3u3eivOE8ALX0BmYUO5JtuRNZ
Dpvt4SAwqCnVUinIf6C+eH/wSurCpapSM0BAHp4aOA7igptyOMgMPYBHNA1e9A7j
E0dCxKWMl3DSWNyjQTk4zeRGEAEfbNjHrq6YCtjHSZSLmWiG80hnfnYos9hOr5Jn
LnyS7ZmFE/5P3XVrxLc/tQ5zum0R4cbrgzHiQP5RgfxGJaEi7XcgherCCOgurJSS
bYH29Gz8u5fFbS+Yg8s+OiCss3cs1rSgJ9/eHZuzGEdUZVARH6hVMjSuwvqVTFaE
8AgtleECgYEA+uLMn4kNqHlJS2A5uAnCkj90ZxEtNm3E8hAxUrhssktY5XSOAPBl
xyf5RuRGIImGtUVIr4HuJSa5TX48n3Vdt9MYCprO/iYl6moNRSPt5qowIIOJmIjY
2mqPDfDt/zw+fcDD3lmCJrFlzcnh0uea1CohxEbQnL3cypeLt+WbU6kCgYEAzSp1
9m1ajieFkqgoB0YTpt/OroDx38vvI5unInJlEeOjQ+oIAQdN2wpxBvTrRorMU6P0
7mFUbt1j+Co6CbNiw+X8HcCaqYLR5clbJOOWNR36PuzOpQLkfK8woupBxzW9B8gZ
mY8rB1mbJ+/WTPrEJy6YGmIEBkWylQ2VpW8O4O0CgYEApdbvvfFBlwD9YxbrcGz7
MeNCFbMz+MucqQntIKoKJ91ImPxvtc0y6e/Rhnv0oyNlaUOwJVu0yNgNG117w0g4
t/+Q38mvVC5xV7/cn7x9UMFk6MkqVir3dYGEqIl/OP1grY2Tq9HtB5iyG9L8NIam
QOLMyUqqMUILxdthHyFmiGkCgYEAn9+PjpjGMPHxL0gj8Q8VbzsFtou6b1deIRRA
2CHmSltltR1gYVTMwXxQeUhPMmgkMqUXzs4/WijgpthY44hK1TaZEKIuoxrS70nJ
4WQLf5a9k1065fDsFZD6yGjdGxvwEmlGMZgTwqV7t1I4X0Ilqhav5hcs5apYL7gn
PYPeRz0CgYALHCj/Ji8XSsDoF/MhVhnGdIs2P99NNdmo3R2Pv0CuZbDKMU559LJH
UvrKS8WkuWRDuKrz1W/EQKApFjDGpdqToZqriUFQzwy7mR3ayIiogzNtHcvbDHx8
oFnGY0OFksX/ye0/XGpy2SFxYRwGU98HPYeBvAQQrVjdkzfy7BmXQQ==
-----END RSA PRIVATE KEY-----"#;

#[test]
fn audience_claim_accepts_string_and_array() {
    #[derive(Deserialize)]
    struct Claims {
        aud: AudienceClaim,
    }

    let string = serde_json::from_value::<Claims>(serde_json::json!({
        "aud": "api://pandar"
    }))
    .unwrap();
    let array = serde_json::from_value::<Claims>(serde_json::json!({
        "aud": ["api://pandar", "api://other"]
    }))
    .unwrap();

    assert_eq!(string.aud, AudienceClaim::One("api://pandar".to_owned()));
    assert_eq!(
        array.aud,
        AudienceClaim::Many(vec!["api://pandar".to_owned(), "api://other".to_owned()])
    );
}

#[tokio::test]
async fn required_scopes_can_be_satisfied_by_scope_string_and_scp_array() {
    let config = config_from_vars([
        (AUDIENCE_VAR, "api://pandar"),
        (REQUIRED_SCOPES_VAR, "print:read,print:write,printer:read"),
    ]);
    let verifier = JwtVerifier::static_jwks(config, jwks());
    let token = token(TestClaims {
        iss: "https://issuer.example.test",
        sub: "user_123",
        exp: jsonwebtoken::get_current_timestamp() + 300,
        aud: Some(AudienceClaimForTest::One("api://pandar".to_owned())),
        scope: Some("print:read print:write"),
        scp: vec!["printer:read".to_owned()],
        email: None,
        email_verified: None,
        name: None,
        preferred_username: None,
    });

    let identity = verifier.verify(&token).await.unwrap();

    assert_eq!(identity.subject, "user_123");
    assert_eq!(
        identity.scopes,
        vec!["print:read", "print:write", "printer:read"]
    );
}

#[tokio::test]
async fn profile_claims_are_extracted_from_valid_jwt() {
    let config = config_from_vars([(AUDIENCE_VAR, "api://pandar")]);
    let verifier = JwtVerifier::static_jwks(config, jwks());
    let token = token(TestClaims {
        iss: "https://issuer.example.test",
        sub: "user_profile",
        exp: jsonwebtoken::get_current_timestamp() + 300,
        aud: Some(AudienceClaimForTest::One("api://pandar".to_owned())),
        scope: None,
        scp: Vec::new(),
        email: Some("alice@example.test"),
        email_verified: Some(true),
        name: Some("Alice Doe"),
        preferred_username: Some("alice"),
    });

    let identity = verifier.verify(&token).await.unwrap();

    assert_eq!(identity.provider, "clerk");
    assert_eq!(identity.subject, "user_profile");
    assert_eq!(identity.email.as_deref(), Some("alice@example.test"));
    assert_eq!(identity.email_verified, Some(true));
    assert_eq!(identity.name.as_deref(), Some("Alice Doe"));
    assert_eq!(identity.preferred_username.as_deref(), Some("alice"));
    assert_eq!(identity.verified_email(), Some("alice@example.test"));
    assert_eq!(identity.display_name(), "Alice Doe");
}

#[tokio::test]
async fn display_name_falls_back_to_username_then_verified_email() {
    let config = config_from_vars([(AUDIENCE_VAR, "api://pandar")]);
    let verifier = JwtVerifier::static_jwks(config, jwks());
    let username_token = token(TestClaims {
        iss: "https://issuer.example.test",
        sub: "user_username",
        exp: jsonwebtoken::get_current_timestamp() + 300,
        aud: Some(AudienceClaimForTest::One("api://pandar".to_owned())),
        scope: None,
        scp: Vec::new(),
        email: Some("username@example.test"),
        email_verified: Some(true),
        name: Some(" "),
        preferred_username: Some("alice"),
    });
    let email_token = token(TestClaims {
        iss: "https://issuer.example.test",
        sub: "user_email",
        exp: jsonwebtoken::get_current_timestamp() + 300,
        aud: Some(AudienceClaimForTest::One("api://pandar".to_owned())),
        scope: None,
        scp: Vec::new(),
        email: Some("alice@example.test"),
        email_verified: Some(true),
        name: None,
        preferred_username: None,
    });

    assert_eq!(
        verifier
            .verify(&username_token)
            .await
            .unwrap()
            .display_name(),
        "alice"
    );
    assert_eq!(
        verifier.verify(&email_token).await.unwrap().display_name(),
        "alice@example.test"
    );
}

fn token(claims: TestClaims) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-key".to_owned());
    encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(TEST_PRIVATE_KEY.as_bytes()).unwrap(),
    )
    .unwrap()
}

fn jwks() -> JwkSet {
    serde_json::from_value(serde_json::json!({
        "keys": [{
            "kty": "RSA",
            "kid": "test-key",
            "alg": "RS256",
            "n": "yRE6rHuNR0QbHO3H3Kt2pOKGVhQqGZXInOduQNxXzuKlvQTLUTv4l4sggh5_CYYi_cvI-SXVT9kPWSKXxJXBXd_4LkvcPuUakBoAkfh-eiFVMh2VrUyWyj3MFl0HTVF9KwRXLAcwkREiS3npThHRyIxuy0ZMeZfxVL5arMhw1SRELB8HoGfG_AtH89BIE9jDBHZ9dLelK9a184zAf8LwoPLxvJb3Il5nncqPcSfKDDodMFBIMc4lQzDKL5gvmiXLXB1AGLm8KBjfE8s3L5xqi-yUod-j8MtvIj812dkS4QMiRVN_by2h3ZY8LYVGrqZXZTcgn2ujn8uKjXLZVD5TdQ",
            "e": "AQAB"
        }]
    }))
    .unwrap()
}

#[derive(Serialize)]
struct TestClaims {
    iss: &'static str,
    sub: &'static str,
    exp: u64,
    aud: Option<AudienceClaimForTest>,
    scope: Option<&'static str>,
    scp: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preferred_username: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum AudienceClaimForTest {
    One(String),
}
