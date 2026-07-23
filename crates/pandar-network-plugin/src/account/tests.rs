use super::{
    canonical_hub_identity, pandar_plugin_account_debug_consistent, pandar_plugin_account_persist,
    persistence,
    types::{PersistedLogin, Profile, ProfileInput, SessionKind, parse_profile},
};

#[cfg(unix)]
use super::types::PendingRevocation;

#[test]
fn hub_identity_ignores_trailing_slashes() {
    assert_eq!(
        canonical_hub_identity("  http://hub.example///  "),
        "http://hub.example"
    );
}

#[test]
fn release_abi_rejects_debug_studio_stl_mode() {
    assert!(pandar_plugin_account_debug_consistent(false));
    assert!(!pandar_plugin_account_debug_consistent(true));
}

#[test]
fn profile_aliases_normalize_to_canonical_typed_shape() {
    let input: ProfileInput = serde_json::from_str(
        r#"{"token":"secret","uidStr":"user-1","name":"A \"quoted\" user","tenant_id":"tenant-1","tenant_name":"Tenant","avatar":"https://example.invalid/a.png"}"#,
    )
    .unwrap();
    let profile = input.normalize().unwrap();
    assert_eq!(profile.user_id, "user-1");
    assert_eq!(profile.user_name, "A \"quoted\" user");
    let encoded = serde_json::to_string(&profile).unwrap();
    let decoded: Profile = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, profile);
}

#[test]
fn minimal_profile_uses_user_id_as_the_studio_display_name() {
    let profile =
        parse_profile(r#"{"token":"next","user_id":"replacement-user"}"#).expect("minimal profile");

    assert_eq!(profile.user_id, "replacement-user");
    assert_eq!(profile.user_name, "replacement-user");
}

#[test]
fn empty_account_session_is_a_persistence_no_op() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_str().unwrap().as_bytes();
    let hub_url = b"http://127.0.0.1:8080";
    let empty = b"";

    let result = pandar_plugin_account_persist(
        config_dir.as_ptr(),
        config_dir.len(),
        hub_url.as_ptr(),
        hub_url.len(),
        empty.as_ptr(),
        empty.len(),
        SessionKind::Authenticated as i32,
        empty.as_ptr(),
        empty.len(),
    );
    let status = result.status;
    let http_code = result.http_code;
    crate::pandar_plugin_free_with_capacity(
        result.body_ptr.cast(),
        result.body_len,
        result.body_cap,
    );

    assert_eq!(status, 0);
    assert_eq!(http_code, 200);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn partially_empty_account_session_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_str().unwrap().as_bytes();
    let hub_url = b"http://127.0.0.1:8080";
    let profile = br#"{"token":"ignored","user_id":"user-1"}"#;
    let empty = b"";
    let token = b"secret-token";

    let persist = |token: &[u8], profile: &[u8]| {
        let result = pandar_plugin_account_persist(
            config_dir.as_ptr(),
            config_dir.len(),
            hub_url.as_ptr(),
            hub_url.len(),
            token.as_ptr(),
            token.len(),
            SessionKind::Authenticated as i32,
            profile.as_ptr(),
            profile.len(),
        );
        let status = result.status;
        let http_code = result.http_code;
        crate::pandar_plugin_free_with_capacity(
            result.body_ptr.cast(),
            result.body_len,
            result.body_cap,
        );
        (status, http_code)
    };

    assert_eq!(persist(empty, profile), (1, 0));
    assert_eq!(persist(token, empty), (1, 0));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn persisted_login_round_trip_replaces_existing_file_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let first = login("first-token", "First User");
    let second = login("second-token", "Second User");
    persistence::store(directory.path().to_str().unwrap(), &first).unwrap();
    persistence::store(directory.path().to_str().unwrap(), &second).unwrap();
    assert_eq!(
        persistence::load(directory.path().to_str().unwrap()).unwrap(),
        Some(second)
    );
    assert!(directory.path().join("pandar-plugin-login.json").is_file());
    for entry in std::fs::read_dir(directory.path()).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        assert!(
            !name.ends_with(".tmp"),
            "atomic write left a temporary account file: {name}"
        );
    }
}

#[cfg(unix)]
#[test]
fn persisted_login_file_is_owner_only_after_create_and_replace() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pandar-plugin-login.json");

    persistence::store(
        directory.path().to_str().unwrap(),
        &login("first-token", "First User"),
    )
    .unwrap();
    let created_mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(
        created_mode & 0o077,
        0,
        "new login file must not grant group or other permissions"
    );

    persistence::store(
        directory.path().to_str().unwrap(),
        &login("second-token", "Second User"),
    )
    .unwrap();
    let replaced_mode = std::fs::metadata(path).unwrap().permissions().mode();
    assert_eq!(
        replaced_mode & 0o077,
        0,
        "replacement login file must not grant group or other permissions"
    );
}

#[cfg(unix)]
#[test]
fn pending_revocation_file_is_owner_only_after_create_and_replace() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_str().unwrap();
    let path = directory
        .path()
        .join("pandar-plugin-pending-revocations.json");

    persistence::enqueue_pending(
        config_dir,
        PendingRevocation {
            hub_url: "http://127.0.0.1:8080".to_owned(),
            token: "first-token".to_owned(),
        },
    )
    .unwrap();
    let created_mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(created_mode & 0o077, 0);

    persistence::enqueue_pending(
        config_dir,
        PendingRevocation {
            hub_url: "http://127.0.0.1:8080".to_owned(),
            token: "second-token".to_owned(),
        },
    )
    .unwrap();
    let replaced_mode = std::fs::metadata(path).unwrap().permissions().mode();
    assert_eq!(replaced_mode & 0o077, 0);
}

#[test]
fn persistence_io_diagnostic_preserves_cause_without_sensitive_path() {
    let directory = tempfile::tempdir().unwrap();
    let marker = "sensitive-login-path-marker-7f340b4e";
    let config_dir = directory.path().join(marker);
    std::fs::create_dir(&config_dir).unwrap();
    std::fs::create_dir(config_dir.join("pandar-plugin-login.json")).unwrap();

    let error = persistence::store(
        config_dir.to_str().unwrap(),
        &login("secret-token", "Sensitive User"),
    )
    .unwrap_err();
    let io_cause = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<std::io::Error>())
        .expect("persistence error must retain its OS cause");
    let diagnostic = format!("{error:#}");

    assert!(diagnostic.contains("atomically replace persisted Studio login"));
    assert!(diagnostic.contains(&io_cause.to_string()));
    assert!(!diagnostic.contains(marker));
    assert!(!diagnostic.contains(&config_dir.display().to_string()));
}

#[test]
fn malformed_persisted_login_preserves_decode_cause() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("pandar-plugin-login.json"),
        "not-json",
    )
    .unwrap();
    let error = persistence::load(directory.path().to_str().unwrap()).unwrap_err();
    let diagnostic = format!("{error:#}");
    assert!(diagnostic.contains("decode persisted Studio login"));
    assert!(diagnostic.contains("expected ident"));
}

fn login(token: &str, user_name: &str) -> PersistedLogin {
    PersistedLogin {
        hub_url: "http://127.0.0.1:8080".to_owned(),
        token: token.to_owned(),
        session_kind: SessionKind::Authenticated,
        profile: Profile {
            user_id: "user-1".to_owned(),
            user_name: user_name.to_owned(),
            tenant_id: "tenant-1".to_owned(),
            tenant_name: "Tenant".to_owned(),
            avatar: String::new(),
        },
    }
}
