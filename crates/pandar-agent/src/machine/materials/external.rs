use super::{
    material_fields, normalize_extruder_toolhead, normalized_string,
    patch::ExternalSpoolPatch,
    schema::{AmsReport, ExternalMaterialSource, MaterialSlotReport, PrintMaterialsReport},
};

pub(super) struct ExternalSpoolsPatch {
    pub(super) spools: Vec<ExternalSpoolPatch>,
    pub(super) replace: bool,
}

pub(super) fn normalize_external_spools(
    print: &PrintMaterialsReport,
) -> Option<ExternalSpoolsPatch> {
    let ams = print.ams.as_ref()?;
    if let Some(vir_slot) = print.vir_slot.as_ref().or(ams.vir_slot.as_ref()) {
        return normalize_external_source(vir_slot, true);
    }
    print
        .vt_tray
        .as_ref()
        .or(ams.vt_tray.as_ref())
        .and_then(|vt_tray| normalize_external_source(vt_tray, false))
}

fn normalize_external_source(
    value: &ExternalMaterialSource,
    vir_slot: bool,
) -> Option<ExternalSpoolsPatch> {
    let (entries, replace_single) = match value {
        ExternalMaterialSource::Array(entries) => (entries, vir_slot),
        ExternalMaterialSource::Object(entry) => return Some(external_source_single(entry, false)),
    };
    if entries.is_empty() {
        return Some(ExternalSpoolsPatch {
            spools: Vec::new(),
            replace: true,
        });
    }

    let multi = entries.len() > 1;
    let spools = entries
        .iter()
        .enumerate()
        .map(|(index, spool)| normalize_external_spool(spool, index, multi))
        .collect();

    Some(ExternalSpoolsPatch {
        spools,
        replace: replace_single || multi,
    })
}

fn external_source_single(spool: &MaterialSlotReport, replace: bool) -> ExternalSpoolsPatch {
    ExternalSpoolsPatch {
        spools: vec![normalize_external_spool(spool, 0, false)],
        replace,
    }
}

fn normalize_external_spool(
    spool: &MaterialSlotReport,
    index: usize,
    multi: bool,
) -> ExternalSpoolPatch {
    ExternalSpoolPatch {
        external_id: normalize_external_id(spool, index, multi),
        exists: true,
        tray_id: if multi {
            index.to_string()
        } else {
            "0".to_owned()
        },
        fields: material_fields(spool),
    }
}

pub(super) fn has_dual_external_slots(print: &PrintMaterialsReport, ams: &AmsReport) -> bool {
    has_dual_external_source(print.vir_slot.as_ref().or(print.vt_tray.as_ref()))
        || has_dual_external_source(ams.vir_slot.as_ref().or(ams.vt_tray.as_ref()))
}

fn has_dual_external_source(source: Option<&ExternalMaterialSource>) -> bool {
    source.is_some_and(|source| match source {
        ExternalMaterialSource::Array(slots) => {
            let has_main = slots
                .iter()
                .any(|slot| external_slot_id(slot).as_deref() == Some("255"));
            let has_deputy = slots
                .iter()
                .any(|slot| external_slot_id(slot).as_deref() == Some("254"));
            has_main && has_deputy
        }
        ExternalMaterialSource::Object(_) => false,
    })
}

fn external_slot_id(slot: &MaterialSlotReport) -> Option<String> {
    normalized_string(slot.id.as_ref()).or_else(|| normalized_string(slot.external_id.as_ref()))
}

fn normalize_external_id(spool: &MaterialSlotReport, index: usize, multi: bool) -> String {
    if let Some(toolhead) = spool
        .toolhead
        .as_ref()
        .and_then(|value| normalized_string(Some(value)))
        .or_else(|| {
            spool
                .extruder_id
                .as_ref()
                .and_then(normalize_extruder_toolhead)
        })
    {
        return external_id_for_toolhead(&toolhead);
    }

    if let Some(id) = spool
        .external_id
        .as_ref()
        .or(spool.id.as_ref())
        .and_then(|value| normalized_string(Some(value)))
        && matches!(id.as_str(), "254" | "255")
    {
        return id;
    }

    if multi && index == 0 {
        "255".to_owned()
    } else {
        "254".to_owned()
    }
}

fn external_id_for_toolhead(toolhead: &str) -> String {
    if toolhead == "L" || toolhead == "l" {
        "254".to_owned()
    } else {
        "255".to_owned()
    }
}
