use std::{error::Error, fmt};

use serde_json::Value;

use crate::machine::{
    BambuPrinterEndpoint, MachineSnapshot,
    materials::normalize_material_patch,
    mqtt::{PrintReportProgress, reports::project_print_report},
};

use super::{DecodedSection, MachineReport, value_payload};
use crate::machine::mqtt::snapshot::{
    NozzleSystemPatch, project_nozzle_system_patch, project_snapshot,
};

#[derive(Debug)]
pub(crate) struct MachineReportInterpretation {
    pub(crate) print: Option<PrintReportProgress>,
    pub(crate) snapshot: Option<MachineSnapshot>,
    pub(crate) materials: Option<NormalizedMaterialPatch>,
    pub(crate) features: DeviceFeatureObservations,
    pub(crate) nozzle_patch: Option<NozzleSystemPatch>,
    pub(crate) facts: MachineReportFacts,
    pub(crate) diagnostics: Vec<MachineReportSectionDiagnostic>,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DeviceFeatureObservations {
    pub(crate) primary: Option<pandar_core::BambuDeviceFeatures>,
    pub(crate) secondary: Option<pandar_core::BambuDeviceFeatures>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedMaterialPatch(String);

impl NormalizedMaterialPatch {
    pub(crate) fn as_json(&self) -> &str {
        &self.0
    }

    pub(crate) fn into_json(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MachineReportFacts {
    pub(crate) print: PrintTelemetryClass,
    pub(crate) snapshot: SnapshotContent,
    pub(crate) authority: SnapshotAuthority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrintTelemetryClass {
    None,
    ProtocolOnly,
    FeatureOnly,
    Operational,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnapshotContent {
    None,
    PrimaryFeatureOnly,
    Telemetry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnapshotAuthority {
    Partial,
    FullPushStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MachineReportSection {
    Print,
    Snapshot,
    Materials,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MachineReportSectionIssue {
    Decode,
    PrimaryDeviceFeatures,
    SecondaryDeviceFeatures,
    Encode,
}

#[derive(Debug)]
pub(crate) struct MachineReportSectionDiagnostic {
    pub(crate) section: MachineReportSection,
    pub(crate) issue: MachineReportSectionIssue,
    pub(crate) source: anyhow::Error,
}

impl fmt::Display for MachineReportSectionDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} failure in {:?} Machine report section: {:#}",
            self.issue, self.section, self.source
        )
    }
}

impl Error for MachineReportSectionDiagnostic {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

impl MachineReportSectionDiagnostic {
    pub(crate) fn is_primary_device_features(&self) -> bool {
        self.issue == MachineReportSectionIssue::PrimaryDeviceFeatures
    }
}

impl MachineReport {
    pub(crate) fn interpret(
        self,
        endpoint: &BambuPrinterEndpoint,
        observed_at: String,
    ) -> MachineReportInterpretation {
        let print_class = classify_print(&self.raw);
        let raw_print = self.raw.get("print").map(value_payload);
        let MachineReport {
            raw: _,
            print,
            snapshot,
            materials,
        } = self;
        let mut diagnostics = Vec::new();

        let mut print = match print {
            DecodedSection::Decoded(envelope)
                if print_class == PrintTelemetryClass::Operational =>
            {
                Some(project_print_report(
                    endpoint,
                    &envelope,
                    raw_print,
                    observed_at.clone(),
                    String::new(),
                ))
            }
            DecodedSection::Decoded(_) => None,
            DecodedSection::Invalid(source) => {
                diagnostics.push(section_diagnostic(
                    MachineReportSection::Print,
                    MachineReportSectionIssue::Decode,
                    source,
                ));
                None
            }
        };

        let (mut snapshot, features, nozzle_patch, authority) = match snapshot {
            DecodedSection::Decoded(report) => {
                let primary =
                    match super::snapshot::parse_primary_device_features(&endpoint.serial, &report)
                    {
                        Ok(value) => value,
                        Err(source) => {
                            diagnostics.push(MachineReportSectionDiagnostic {
                                section: MachineReportSection::Snapshot,
                                issue: MachineReportSectionIssue::PrimaryDeviceFeatures,
                                source,
                            });
                            None
                        }
                    };
                let secondary = match super::snapshot::parse_secondary_device_features(
                    &endpoint.serial,
                    &report,
                ) {
                    Ok(value) => value,
                    Err(source) => {
                        diagnostics.push(MachineReportSectionDiagnostic {
                            section: MachineReportSection::Snapshot,
                            issue: MachineReportSectionIssue::SecondaryDeviceFeatures,
                            source,
                        });
                        None
                    }
                };
                let features = DeviceFeatureObservations { primary, secondary };
                let nozzle_patch = project_nozzle_system_patch(Some(&report));
                let authority = if report.is_full_push_status() {
                    SnapshotAuthority::FullPushStatus
                } else {
                    SnapshotAuthority::Partial
                };
                let snapshot =
                    project_snapshot(endpoint, Some(&report), features, nozzle_patch.as_ref());
                (Some(snapshot), features, nozzle_patch, authority)
            }
            DecodedSection::Invalid(source) => {
                diagnostics.push(section_diagnostic(
                    MachineReportSection::Snapshot,
                    MachineReportSectionIssue::Decode,
                    source,
                ));
                (
                    None,
                    DeviceFeatureObservations::default(),
                    None,
                    SnapshotAuthority::Partial,
                )
            }
        };

        let materials = match materials {
            DecodedSection::Decoded(report) => {
                match normalize_material_patch(&report, &observed_at) {
                    Some(patch) => match serde_json::to_string(&patch) {
                        Ok(json) => Some(NormalizedMaterialPatch(json)),
                        Err(source) => {
                            diagnostics.push(section_diagnostic(
                                MachineReportSection::Materials,
                                MachineReportSectionIssue::Encode,
                                source,
                            ));
                            None
                        }
                    },
                    None => None,
                }
            }
            DecodedSection::Invalid(source) => {
                diagnostics.push(section_diagnostic(
                    MachineReportSection::Materials,
                    MachineReportSectionIssue::Decode,
                    source,
                ));
                None
            }
        };

        if let Some(progress) = &mut print
            && let Some(materials) = &materials
        {
            progress.printer_materials_json = materials.as_json().to_owned();
        }

        let snapshot_content = snapshot.as_ref().map_or(SnapshotContent::None, |value| {
            snapshot_content(value, authority)
        });
        if let Some(snapshot) = &mut snapshot {
            snapshot.telemetry_authoritative = authority == SnapshotAuthority::FullPushStatus;
        }

        MachineReportInterpretation {
            print,
            snapshot,
            materials,
            features,
            nozzle_patch,
            facts: MachineReportFacts {
                print: print_class,
                snapshot: snapshot_content,
                authority,
            },
            diagnostics,
        }
    }
}

fn section_diagnostic(
    section: MachineReportSection,
    issue: MachineReportSectionIssue,
    source: serde_json::Error,
) -> MachineReportSectionDiagnostic {
    MachineReportSectionDiagnostic {
        section,
        issue,
        source: source.into(),
    }
}

fn classify_print(raw: &Value) -> PrintTelemetryClass {
    let Some(print) = raw.get("print").and_then(Value::as_object) else {
        return PrintTelemetryClass::None;
    };
    if raw.as_object().is_some_and(|fields| fields.len() == 1)
        && print.len() == 1
        && (print.contains_key("fun") || print.contains_key("fun2"))
    {
        return PrintTelemetryClass::FeatureOnly;
    }
    if print
        .keys()
        .any(|key| !matches!(key.as_str(), "command" | "msg" | "cfg" | "upgrade_state"))
    {
        PrintTelemetryClass::Operational
    } else {
        PrintTelemetryClass::ProtocolOnly
    }
}

fn snapshot_content(snapshot: &MachineSnapshot, authority: SnapshotAuthority) -> SnapshotContent {
    if authority == SnapshotAuthority::FullPushStatus
        || snapshot.state.is_some()
        || !snapshot.nozzle_temperatures.is_empty()
        || snapshot.active_nozzle.is_some()
        || snapshot.bed_temperature_celsius.is_some()
        || snapshot.bed_target_temperature_celsius.is_some()
        || snapshot.chamber_temperature_celsius.is_some()
        || snapshot.chamber_target_temperature_celsius.is_some()
        || snapshot.chamber_light_on.is_some()
        || snapshot.cooling_system.is_some()
        || snapshot.device_features2.is_some()
        || snapshot.nozzle_system.is_some()
    {
        SnapshotContent::Telemetry
    } else if snapshot.device_features.is_some() {
        SnapshotContent::PrimaryFeatureOnly
    } else {
        SnapshotContent::None
    }
}
