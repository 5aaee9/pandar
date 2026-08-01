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
    pub bind_model_argument: bool,
    pub ams_sync: bool,
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

    pub fn reference_version_components(&self) -> [u16; 4] {
        parse_version(&self.reference_studio_version).expect("validated reference Studio version")
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
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_contains_supported_abi_series() {
        let catalog = catalog();

        assert_eq!(catalog.default_abi_series, "02.07.01");
        assert_eq!(catalog.abi_series.len(), 6);
        assert_eq!(catalog.abi_series("02.06.00").unwrap().total_exports(), 124);
        assert_eq!(catalog.abi_series("02.08.00").unwrap().total_exports(), 129);
        assert_eq!(catalog.abi_series("02.08.01").unwrap().total_exports(), 130);
        assert!(
            !catalog
                .abi_series("02.06.00")
                .unwrap()
                .capabilities
                .filament_cloud
        );
        assert!(
            catalog
                .abi_series("02.06.01")
                .unwrap()
                .capabilities
                .filament_cloud
        );
        assert!(
            !catalog
                .abi_series("02.07.00")
                .unwrap()
                .capabilities
                .print_svc_context
        );
        assert!(
            catalog
                .abi_series("02.07.01")
                .unwrap()
                .capabilities
                .print_svc_context
        );
        assert!(
            !catalog
                .abi_series("02.07.01")
                .unwrap()
                .capabilities
                .bind_model_argument
        );
        assert!(
            catalog
                .abi_series("02.08.00")
                .unwrap()
                .capabilities
                .bind_model_argument
        );
        let studio_2_8_1 = &catalog.abi_series("02.08.01").unwrap().capabilities;
        assert!(studio_2_8_1.print_slicer_uid);
        assert!(studio_2_8_1.ams_sync);
    }

    #[test]
    fn embedded_catalog_matches_reference_snapshots() {
        let expected = [
            (
                "02.06.00",
                "02.06.00.51",
                "b506005bc4ee62124e24bf00e0f58656db3646a6",
                "02.06.00.50",
                103,
            ),
            (
                "02.06.01",
                "02.06.01.55",
                "6eb52d6ac75e32ba2116239c1d756d913053f364",
                "02.06.01.50",
                108,
            ),
            (
                "02.07.00",
                "02.07.00.55",
                "4410c27fb15d57b29fbb1dbebc6edea11a091137",
                "02.06.01.50",
                108,
            ),
            (
                "02.07.01",
                "02.07.01.57",
                "3f126b717ed1f10fee0f32f05ed9731808d0c8bb",
                "02.07.01.51",
                108,
            ),
            (
                "02.08.00",
                "02.08.00.50",
                "a78684a11de4abddad9a6d19eeb75a6a1d2e82a5",
                "02.08.00.53",
                108,
            ),
            (
                "02.08.01",
                "02.08.01.55",
                "ba049f6a2e08c3b6033660bb84da80c08722974b",
                "02.08.01.52",
                109,
            ),
        ];

        for (id, reference_version, commit, agent_version, network_exports) in expected {
            let series = abi_series(id).unwrap();
            assert_eq!(series.reference_studio_version, reference_version);
            assert_eq!(series.studio_commit, commit);
            assert_eq!(series.reference_network_agent_version, agent_version);
            assert_eq!(series.reported_network_agent_version, format!("{id}.99"));
            assert_eq!(series.network_exports, network_exports);
            assert_eq!(series.file_transfer_exports, 21);
            assert_eq!(resolve_studio_version(reference_version).unwrap().id, id);
        }
    }

    #[test]
    fn resolves_four_part_studio_versions_by_first_three_components() {
        assert_eq!(
            resolve_studio_version("02.07.01.62").unwrap().id,
            "02.07.01"
        );
        assert_eq!(resolve_studio_version("2.7.1.62").unwrap().id, "02.07.01");
        assert_eq!(
            resolve_studio_version("02.07.01.99").unwrap().id,
            "02.07.01"
        );
        assert_eq!(
            resolve_studio_version("02.08.01.55").unwrap().id,
            "02.08.01"
        );
        assert!(resolve_studio_version("02.09.00.00").is_err());
        assert!(resolve_studio_version("02.07.01").is_err());
    }

    #[test]
    fn enables_ams_contract_mode_only_for_studio_2_8_1() {
        assert_eq!(
            abi_series("02.08.00").unwrap().native_modes(),
            ["version", "bind", "print", "ft"]
        );
        assert_eq!(
            abi_series("02.08.01").unwrap().native_modes(),
            ["version", "bind", "print", "ams", "ft"]
        );
    }

    #[test]
    fn selects_release_assets_by_abi_series() {
        assert_eq!(
            abi_series("02.07.01").unwrap().hook_bundle_name(),
            "pandar-studio-hook-02.07.01-windows-amd64.zip"
        );
        assert_eq!(
            abi_series("02.08.00").unwrap().hook_bundle_name(),
            "pandar-studio-hook-02.08.00-windows-amd64.zip"
        );
        assert_eq!(
            abi_series("02.08.01").unwrap().hook_bundle_name(),
            "pandar-studio-hook-02.08.01-windows-amd64.zip"
        );
    }

    #[test]
    fn rejects_unknown_or_duplicate_abi_series() {
        assert!(catalog().abi_series("02.09.00").is_err());
        let duplicate = ABI_SERIES_MANIFEST.replace("\"02.08.00\"", "\"02.07.01\"");
        assert!(StudioAbiSeriesCatalog::parse(&duplicate).is_err());
    }
}
