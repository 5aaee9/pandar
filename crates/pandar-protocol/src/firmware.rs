use pandar_core::{
    AmsFirmwareDescriptor, AmsFirmwareSwitchState, PrinterFirmwareModule, PrinterFirmwareVersion,
    PrinterUpgradeState,
};

use crate::agent::v1::{
    AmsFirmwareDescriptor as ProtoAmsDescriptor,
    AmsFirmwareDescriptorList as ProtoAmsDescriptorList,
    AmsFirmwareSwitchState as ProtoAmsSwitchState, PrinterFirmwareModule as ProtoFirmwareModule,
    PrinterFirmwareVersion as ProtoFirmwareVersion, PrinterFirmwareVersionList as ProtoVersionList,
    PrinterUpgradeState as ProtoUpgradeState,
};

/// Convert a persisted firmware module into its wire form (agent → hub).
pub fn proto_module(module: PrinterFirmwareModule) -> ProtoFirmwareModule {
    ProtoFirmwareModule {
        name: module.name,
        software_version: module.software_version,
        software_new_version: module.software_new_version,
        new_version: module.new_version,
        visible: module.visible,
        product_name: module.product_name,
        serial_number: module.serial_number,
        hardware_version: module.hardware_version,
        firmware_flag: module.firmware_flag,
    }
}

/// Convert a wire firmware module into its persisted form (hub ← agent).
pub fn core_module(module: ProtoFirmwareModule) -> PrinterFirmwareModule {
    PrinterFirmwareModule {
        name: module.name,
        software_version: module.software_version,
        software_new_version: module.software_new_version,
        new_version: module.new_version,
        visible: module.visible,
        product_name: module.product_name,
        serial_number: module.serial_number,
        hardware_version: module.hardware_version,
        firmware_flag: module.firmware_flag,
    }
}

/// Convert a persisted upgrade state into its wire form (agent → hub).
pub fn proto_upgrade_state(state: PrinterUpgradeState) -> ProtoUpgradeState {
    ProtoUpgradeState {
        status: state.status,
        progress: state.progress,
        message: state.message,
        module: state.module,
        error_code: state.error_code,
        new_version_state: state.new_version_state,
        consistency_request: state.consistency_request,
        force_upgrade: state.force_upgrade,
        display_state: state.display_state,
        ota_new_version_number: state.ota_new_version_number,
        ams_new_version_number: state.ams_new_version_number,
        ahb_new_version_number: state.ahb_new_version_number,
        new_versions: state.new_versions.map(|versions| ProtoVersionList {
            versions: versions
                .into_iter()
                .map(|version| ProtoFirmwareVersion {
                    name: version.name,
                    current_version: version.current_version,
                    new_version: version.new_version,
                })
                .collect(),
        }),
        ams_firmware: state.ams_firmware.map(|ams| ProtoAmsSwitchState {
            firmware: ams.firmware.map(|firmware| ProtoAmsDescriptorList {
                firmware: firmware
                    .into_iter()
                    .map(|entry| ProtoAmsDescriptor {
                        id: entry.id,
                        name: entry.name,
                        version: entry.version,
                    })
                    .collect(),
            }),
            current_firmware_id: ams.current_firmware_id,
            current_run_firmware_id: ams.current_run_firmware_id,
            status: ams.status,
        }),
    }
}

/// Convert a wire upgrade state into its persisted form (hub ← agent).
pub fn core_upgrade_state(state: ProtoUpgradeState) -> PrinterUpgradeState {
    PrinterUpgradeState {
        status: state.status,
        progress: state.progress,
        message: state.message,
        module: state.module,
        error_code: state.error_code,
        new_version_state: state.new_version_state,
        consistency_request: state.consistency_request,
        force_upgrade: state.force_upgrade,
        display_state: state.display_state,
        ota_new_version_number: state.ota_new_version_number,
        ams_new_version_number: state.ams_new_version_number,
        ahb_new_version_number: state.ahb_new_version_number,
        new_versions: state.new_versions.map(|versions| {
            versions
                .versions
                .into_iter()
                .map(|version| PrinterFirmwareVersion {
                    name: version.name,
                    current_version: version.current_version,
                    new_version: version.new_version,
                })
                .collect()
        }),
        ams_firmware: state.ams_firmware.map(|ams| AmsFirmwareSwitchState {
            firmware: ams.firmware.map(|firmware| {
                firmware
                    .firmware
                    .into_iter()
                    .map(|entry| AmsFirmwareDescriptor {
                        id: entry.id,
                        name: entry.name,
                        version: entry.version,
                    })
                    .collect()
            }),
            current_firmware_id: ams.current_firmware_id,
            current_run_firmware_id: ams.current_run_firmware_id,
            status: ams.status,
        }),
    }
}
