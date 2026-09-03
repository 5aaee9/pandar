use std::{fs, path::Path, sync::Mutex};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::normalize_hub_url;

use super::{
    persistence::{MutationDurability, acquire_process_lock, durable_write_replace},
    runtime::canonical_hub_identity,
};

const SERVER_SELECTION_FILE: &str = "pandar-plugin-server-selection.json";

static SELECTION_FILE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(super) struct PersistedServerSelection {
    pub(super) web_url: String,
    pub(super) hub_url: String,
}

impl PersistedServerSelection {
    /// Canonicalizes a manually selected Web URL and its discovered Hub identity.
    /// Returns `None` when either URL cannot form a canonical identity.
    pub(super) fn new(web_url: String, hub_url: String) -> Option<Self> {
        let web_url = normalize_hub_url(web_url)?;
        let hub_url = canonical_hub_identity(&hub_url);
        (!hub_url.is_empty()).then_some(Self { web_url, hub_url })
    }
}

pub(super) fn load(config_dir: &str) -> anyhow::Result<Option<PersistedServerSelection>> {
    let _process_guard = acquire_process_lock(config_dir)?;
    let _guard = SELECTION_FILE_LOCK
        .lock()
        .expect("server selection file lock");
    load_unlocked(config_dir)
}

fn load_unlocked(config_dir: &str) -> anyhow::Result<Option<PersistedServerSelection>> {
    if config_dir.is_empty() {
        return Ok(None);
    }
    let path = Path::new(config_dir).join(SERVER_SELECTION_FILE);
    let body = match fs::read_to_string(&path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("read saved Pandar server selection"),
    };
    serde_json::from_str(&body).context("decode saved Pandar server selection")
}

pub(super) fn store(
    config_dir: &str,
    selection: &PersistedServerSelection,
) -> anyhow::Result<MutationDurability> {
    let _process_guard = acquire_process_lock(config_dir)?;
    let _guard = SELECTION_FILE_LOCK
        .lock()
        .expect("server selection file lock");
    store_unlocked(config_dir, selection)
}

fn store_unlocked(
    config_dir: &str,
    selection: &PersistedServerSelection,
) -> anyhow::Result<MutationDurability> {
    if config_dir.is_empty() {
        return Ok(MutationDurability::Confirmed);
    }
    let directory = Path::new(config_dir);
    fs::create_dir_all(directory).context("create saved Pandar server selection directory")?;
    let path = directory.join(SERVER_SELECTION_FILE);
    let body = serde_json::to_vec(selection).context("encode saved Pandar server selection")?;
    Ok(durable_write_replace(directory, &path, &body)?.reconfirm(
        directory,
        "confirm saved Pandar server selection replacement",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection() -> PersistedServerSelection {
        PersistedServerSelection {
            web_url: "https://pandar-web.example.test".to_owned(),
            hub_url: "https://pandar-hub.example.test".to_owned(),
        }
    }

    #[test]
    fn store_and_load_round_trips_typed_selection() {
        let directory = tempfile::tempdir().unwrap();
        let config_dir = directory.path().to_string_lossy().into_owned();

        store(&config_dir, &selection())
            .unwrap()
            .require_confirmed("test")
            .unwrap();

        assert_eq!(load(&config_dir).unwrap(), Some(selection()));
        let body =
            std::fs::read_to_string(directory.path().join("pandar-plugin-server-selection.json"))
                .unwrap();
        assert_eq!(
            body,
            r#"{"web_url":"https://pandar-web.example.test","hub_url":"https://pandar-hub.example.test"}"#
        );
    }

    #[test]
    fn canonical_identity_drops_trailing_slashes_and_rejects_unsafe_urls() {
        let canonical = PersistedServerSelection::new(
            "https://pandar-web.example.test/".to_owned(),
            "https://pandar-hub.example.test/".to_owned(),
        )
        .unwrap();
        assert_eq!(
            canonical,
            PersistedServerSelection {
                web_url: "https://pandar-web.example.test".to_owned(),
                hub_url: "https://pandar-hub.example.test".to_owned(),
            }
        );
        assert!(
            PersistedServerSelection::new(
                "ftp://pandar-web.example.test".to_owned(),
                "https://pandar-hub.example.test".to_owned(),
            )
            .is_none()
        );
        assert!(
            PersistedServerSelection::new(
                "https://pandar-web.example.test".to_owned(),
                String::new(),
            )
            .is_none()
        );
    }

    #[test]
    fn malformed_selection_fails_to_decode_with_context() {
        let directory = tempfile::tempdir().unwrap();
        let config_dir = directory.path().to_string_lossy().into_owned();
        std::fs::write(
            directory.path().join("pandar-plugin-server-selection.json"),
            "{\"web_url\":",
        )
        .unwrap();

        let error = load(&config_dir).unwrap_err();
        assert!(
            format!("{error:#}").contains("decode saved Pandar server selection"),
            "missing decode context: {error:#}"
        );
    }

    #[test]
    fn missing_selection_and_empty_config_dir_load_none() {
        let directory = tempfile::tempdir().unwrap();
        let config_dir = directory.path().to_string_lossy().into_owned();
        assert_eq!(load(&config_dir).unwrap(), None);
        assert_eq!(load("").unwrap(), None);
    }
}
