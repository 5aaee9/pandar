use std::sync::OnceLock;

use serde::Deserialize;

pub const ABI_SERIES_ENV: &str = "PANDAR_STUDIO_ABI_SERIES";
pub const ABI_SERIES_MANIFEST: &str = include_str!("../../../studio-abi-profiles.json");

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct StudioAbiSeriesCatalog {
    pub schema_version: u32,
    pub default_abi_series: String,
    pub abi_series: Vec<StudioAbiSeries>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct StudioAbiSeries {
    pub id: String,
    pub reference_studio_version: String,
    pub studio_commit: String,
    pub reference_network_agent_version: String,
    pub reported_network_agent_version: String,
    pub network_exports: usize,
    pub file_transfer_exports: usize,
    pub capabilities: StudioAbiCapabilities,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub struct StudioAbiCapabilities {
    pub filament_cloud: bool,
    pub print_svc_context: bool,
    pub print_slicer_uid: bool,
    pub print_queue_plate_id: bool,
    pub bind_model_argument: bool,
    pub ams_sync: bool,
    pub slot_mappings_sync: bool,
}

impl StudioAbiSeriesCatalog {
    pub fn parse(json: &str) -> Result<Self, String> {
        let catalog = serde_json::from_str::<Self>(json)
            .map_err(|error| format!("parse Studio ABI series catalog: {error}"))?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn abi_series(&self, id: &str) -> Result<&StudioAbiSeries, String> {
        let id = normalize_abi_series(id)
            .map_err(|_| format!("invalid Bambu Studio ABI series {id}"))?;
        self.abi_series
            .iter()
            .find(|series| series.id == id)
            .ok_or_else(|| format!("unsupported Bambu Studio ABI series {id}"))
    }

    pub fn resolve_studio_version(&self, version: &str) -> Result<&StudioAbiSeries, String> {
        let id = abi_series_id_for_studio_version(version)?;
        self.abi_series(&id)
            .map_err(|_| format!("unsupported Bambu Studio version {version} (ABI series {id})"))
    }

    pub fn default(&self) -> &StudioAbiSeries {
        self.abi_series(&self.default_abi_series)
            .expect("validated default Studio ABI series exists")
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 2 {
            return Err(format!(
                "unsupported Studio ABI series schema version {}",
                self.schema_version
            ));
        }
        if self.abi_series.is_empty() {
            return Err("Studio ABI series catalog is empty".to_owned());
        }
        for (index, series) in self.abi_series.iter().enumerate() {
            let normalized_id = normalize_abi_series(&series.id)
                .map_err(|_| format!("invalid Studio ABI series id {}", series.id))?;
            if normalized_id != series.id {
                return Err(format!(
                    "Studio ABI series id must be canonical: {}",
                    series.id
                ));
            }
            let reference = parse_version::<4>(&series.reference_studio_version).map_err(|_| {
                format!(
                    "invalid reference Studio version for ABI series {}",
                    series.id
                )
            })?;
            let reference_series = format!(
                "{:02}.{:02}.{:02}",
                reference[0], reference[1], reference[2]
            );
            if reference_series != series.id {
                return Err(format!(
                    "reference Studio version {} does not belong to ABI series {}",
                    series.reference_studio_version, series.id
                ));
            }
            if series.studio_commit.len() != 40
                || !series
                    .studio_commit
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(format!(
                    "invalid Studio commit for ABI series {}",
                    series.id
                ));
            }
            parse_version::<4>(&series.reference_network_agent_version).map_err(|_| {
                format!(
                    "invalid reference network agent version for ABI series {}",
                    series.id
                )
            })?;
            let reported_series = abi_series_id_for_studio_version(
                &series.reported_network_agent_version,
            )
            .map_err(|_| {
                format!(
                    "invalid reported network agent version for ABI series {}",
                    series.id
                )
            })?;
            if reported_series != series.id {
                return Err(format!(
                    "reported network agent version {} does not belong to ABI series {}",
                    series.reported_network_agent_version, series.id
                ));
            }
            if series.file_transfer_exports == 0 || series.network_exports == 0 {
                return Err(format!(
                    "empty export contract for ABI series {}",
                    series.id
                ));
            }
            if self.abi_series[..index]
                .iter()
                .any(|earlier| earlier.id == series.id)
            {
                return Err(format!("duplicate Studio ABI series {}", series.id));
            }
        }
        self.abi_series(&self.default_abi_series)?;
        Ok(())
    }
}

impl StudioAbiSeries {
    pub fn series_components(&self) -> [u16; 3] {
        parse_version(&self.id).expect("validated Studio ABI series version")
    }

    pub fn total_exports(&self) -> usize {
        self.network_exports + self.file_transfer_exports
    }

    pub fn native_modes(&self) -> &'static [&'static str] {
        if self.capabilities.slot_mappings_sync {
            &["version", "bind", "print", "ams", "slot-mappings", "ft"]
        } else if self.capabilities.ams_sync {
            &["version", "bind", "print", "ams", "ft"]
        } else {
            &["version", "bind", "print", "ft"]
        }
    }

    pub fn hook_bundle_name(&self) -> String {
        format!("pandar-studio-hook-{}-windows-amd64.zip", self.id)
    }
}

pub fn catalog() -> &'static StudioAbiSeriesCatalog {
    static CATALOG: OnceLock<StudioAbiSeriesCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        StudioAbiSeriesCatalog::parse(ABI_SERIES_MANIFEST)
            .expect("embedded Studio ABI series catalog is valid")
    })
}

pub fn abi_series(id: &str) -> Result<&'static StudioAbiSeries, String> {
    catalog().abi_series(id)
}

pub fn resolve_studio_version(version: &str) -> Result<&'static StudioAbiSeries, String> {
    catalog().resolve_studio_version(version)
}

pub fn abi_series_id_for_studio_version(version: &str) -> Result<String, String> {
    let components = parse_version::<4>(version)
        .map_err(|_| format!("invalid Bambu Studio version {version}"))?;
    Ok(format!(
        "{:02}.{:02}.{:02}",
        components[0], components[1], components[2]
    ))
}

pub fn abi_series_from_env() -> Result<&'static StudioAbiSeries, String> {
    match std::env::var(ABI_SERIES_ENV) {
        Ok(id) => abi_series(&id),
        Err(std::env::VarError::NotPresent) => Ok(catalog().default()),
        Err(error) => Err(format!("read {ABI_SERIES_ENV}: {error}")),
    }
}

fn normalize_abi_series(series: &str) -> Result<String, ()> {
    let [major, minor, patch] = parse_version::<3>(series)?;
    Ok(format!("{major:02}.{minor:02}.{patch:02}"))
}

fn parse_version<const N: usize>(version: &str) -> Result<[u16; N], ()> {
    let values = version
        .split('.')
        .map(|part| part.parse::<u16>().map_err(|_| ()))
        .collect::<Result<Vec<_>, _>>()?;
    values.try_into().map_err(|_| ())
}

#[cfg(test)]
mod tests;
