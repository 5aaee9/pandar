use std::sync::OnceLock;

use serde::Deserialize;

pub const PROFILE_ENV: &str = "PANDAR_STUDIO_PROFILE";
pub const PROFILE_MANIFEST: &str = include_str!("../../../studio-abi-profiles.json");

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct StudioProfileCatalog {
    pub schema_version: u32,
    pub default_profile: String,
    pub profiles: Vec<StudioProfile>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct StudioProfile {
    pub id: String,
    pub studio_commit: String,
    pub network_agent_version: String,
    pub network_exports: usize,
    pub file_transfer_exports: usize,
    pub capabilities: StudioCapabilities,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub struct StudioCapabilities {
    pub bind_model_argument: bool,
    pub print_slicer_uid: bool,
    pub ams_sync: bool,
}

impl StudioProfileCatalog {
    pub fn parse(json: &str) -> Result<Self, String> {
        let catalog = serde_json::from_str::<Self>(json)
            .map_err(|error| format!("parse Studio ABI profile catalog: {error}"))?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn profile(&self, id: &str) -> Result<&StudioProfile, String> {
        self.profiles
            .iter()
            .find(|profile| profile.id == id)
            .ok_or_else(|| format!("unsupported Bambu Studio ABI profile {id}"))
    }

    pub fn default(&self) -> &StudioProfile {
        self.profile(&self.default_profile)
            .expect("validated default Studio ABI profile exists")
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "unsupported Studio ABI profile schema version {}",
                self.schema_version
            ));
        }
        if self.profiles.is_empty() {
            return Err("Studio ABI profile catalog is empty".to_owned());
        }
        for (index, profile) in self.profiles.iter().enumerate() {
            if parse_version(&profile.id).is_err() {
                return Err(format!("invalid Studio ABI profile id {}", profile.id));
            }
            if profile.studio_commit.len() != 40
                || !profile
                    .studio_commit
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(format!("invalid Studio commit for profile {}", profile.id));
            }
            parse_version(&profile.network_agent_version)
                .map_err(|_| format!("invalid network agent version for profile {}", profile.id))?;
            if profile.file_transfer_exports == 0 || profile.network_exports == 0 {
                return Err(format!("empty export contract for profile {}", profile.id));
            }
            if self.profiles[..index]
                .iter()
                .any(|earlier| earlier.id == profile.id)
            {
                return Err(format!("duplicate Studio ABI profile {}", profile.id));
            }
        }
        self.profile(&self.default_profile)?;
        Ok(())
    }
}

impl StudioProfile {
    pub fn version_components(&self) -> [u16; 4] {
        parse_version(&self.id).expect("validated Studio ABI profile version")
    }

    pub fn total_exports(&self) -> usize {
        self.network_exports + self.file_transfer_exports
    }

    pub fn native_modes(&self) -> &'static [&'static str] {
        if self.capabilities.ams_sync {
            &["version", "bind", "print", "ams", "ft"]
        } else {
            &["version", "bind", "print", "ft"]
        }
    }

    pub fn hook_bundle_name(&self) -> String {
        format!("pandar-studio-hook-{}-windows-amd64.zip", self.id)
    }
}

pub fn catalog() -> &'static StudioProfileCatalog {
    static CATALOG: OnceLock<StudioProfileCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        StudioProfileCatalog::parse(PROFILE_MANIFEST)
            .expect("embedded Studio ABI profile catalog is valid")
    })
}

pub fn profile(id: &str) -> Result<&'static StudioProfile, String> {
    catalog().profile(id)
}

pub fn profile_from_env() -> Result<&'static StudioProfile, String> {
    match std::env::var(PROFILE_ENV) {
        Ok(id) => profile(&id),
        Err(std::env::VarError::NotPresent) => Ok(catalog().default()),
        Err(error) => Err(format!("read {PROFILE_ENV}: {error}")),
    }
}

fn parse_version(version: &str) -> Result<[u16; 4], ()> {
    let values = version
        .split('.')
        .map(|part| part.parse::<u16>().map_err(|_| ()))
        .collect::<Result<Vec<_>, _>>()?;
    values.try_into().map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_contains_exact_supported_profiles() {
        let catalog = catalog();

        assert_eq!(catalog.default_profile, "02.07.01.62");
        assert_eq!(catalog.profiles.len(), 2);
        assert_eq!(catalog.profile("02.07.01.62").unwrap().total_exports(), 129);
        assert_eq!(catalog.profile("02.08.01.55").unwrap().total_exports(), 130);
        assert!(
            !catalog
                .profile("02.07.01.62")
                .unwrap()
                .capabilities
                .ams_sync
        );
        assert!(
            catalog
                .profile("02.08.01.55")
                .unwrap()
                .capabilities
                .ams_sync
        );
    }

    #[test]
    fn rejects_unknown_or_duplicate_profiles() {
        assert!(catalog().profile("02.09.00.00").is_err());
        let duplicate = PROFILE_MANIFEST.replace("\"02.08.01.55\"", "\"02.07.01.62\"");
        assert!(StudioProfileCatalog::parse(&duplicate).is_err());
    }
}
