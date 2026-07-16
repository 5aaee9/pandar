use serde::Serialize;

use super::{MaterialJsonValue, MaterialSnapshot};

impl MaterialSnapshot {
    pub(crate) fn persisted_json(&self) -> String {
        serde_json::to_string(&PersistedMaterialSnapshotJson {
            ams_units: &self.ams_units,
            external_spools: &self.external_spools,
            active_tray: self.active_tray.as_ref(),
            filament_switch_installed: self.filament_switch_installed,
            cfg: self.cfg.as_deref(),
            aux: self.aux.as_deref(),
            stat: self.stat.as_deref(),
        })
        .expect("persisted material snapshot JSON is serializable")
    }
}

#[derive(Serialize)]
struct PersistedMaterialSnapshotJson<'a> {
    ams_units: &'a MaterialJsonValue,
    external_spools: &'a MaterialJsonValue,
    active_tray: Option<&'a MaterialJsonValue>,
    filament_switch_installed: Option<bool>,
    cfg: Option<&'a str>,
    aux: Option<&'a str>,
    stat: Option<&'a str>,
}
