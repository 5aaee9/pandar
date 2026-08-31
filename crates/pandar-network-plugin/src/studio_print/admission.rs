use std::path::{Path, PathBuf};

use pandar_core::{
    PrintCalibrationMode, PrintTransferFailure, StudioAmsMappingEntry, StudioAmsMappingInfo,
    StudioNozzleInfo,
};
use serde::{Serialize, de::DeserializeOwned};

use super::{
    diagnostics::diagnose_json,
    ffi::{PluginStudioPrintParams, PluginStudioSnapshot},
};

pub(super) struct AdmittedPrint {
    pub(super) hub_url: String,
    pub(super) token: String,
    pub(super) printer_id: String,
    pub(super) account_epoch: u64,
    pub(super) cache_generation: u64,
    pub(super) firmware_generation: u64,
    pub(super) task_name: String,
    pub(super) project_name: String,
    pub(super) preset_name: String,
    pub(super) artifact_path: PathBuf,
    pub(super) artifact_filename: String,
    pub(super) config_filename: String,
    pub(super) config_plate_index: Option<u32>,
    pub(super) plate_index: u32,
    pub(super) nozzle_mapping: Vec<i32>,
    pub(super) ams_mapping: Vec<i32>,
    pub(super) ams_mapping2: Vec<StudioAmsMappingEntry>,
    pub(super) ams_mapping_info: Vec<StudioAmsMappingInfo>,
    pub(super) nozzles_info: Vec<StudioNozzleInfo>,
    pub(super) connection_type: String,
    pub(super) comments: String,
    pub(super) origin_profile_id: i64,
    pub(super) stl_design_id: i64,
    pub(super) origin_model_id: String,
    pub(super) print_type: String,
    pub(super) dev_name: String,
    pub(super) bed_leveling: bool,
    pub(super) flow_cali: bool,
    pub(super) vibration_cali: bool,
    pub(super) layer_inspect: bool,
    pub(super) timelapse: bool,
    pub(super) timelapse_use_internal: bool,
    pub(super) use_ams: bool,
    pub(super) bed_type: String,
    pub(super) auto_bed_leveling: PrintCalibrationMode,
    pub(super) auto_flow_cali: PrintCalibrationMode,
    pub(super) auto_offset_cali: PrintCalibrationMode,
    pub(super) extruder_cali_manual_mode: i8,
    pub(super) try_emmc_print: bool,
    pub(super) svc_context: String,
    pub(super) slicer_uid: String,
}

#[derive(Clone)]
pub(super) struct PrintFailure {
    pub(super) code: i32,
    pub(super) body: String,
}

#[derive(Serialize)]
struct FieldFailure<'a> {
    error: &'a str,
    field: &'a str,
}

#[derive(Serialize)]
struct SimpleFailure<'a> {
    error: &'a str,
}

#[derive(Serialize)]
struct TransferFailure<'a> {
    error: &'static str,
    #[serde(flatten)]
    failure: &'a PrintTransferFailure,
}

impl PrintFailure {
    pub(super) fn invalid(field: &'static str) -> Self {
        Self::field("invalid_print_param", field)
    }

    pub(super) fn unsupported(field: &'static str) -> Self {
        Self::field("unsupported_print_param", field)
    }

    pub(super) fn simple(error: &'static str) -> Self {
        Self {
            code: -19,
            body: serde_json::to_string(&SimpleFailure { error })
                .expect("print error body is serializable"),
        }
    }

    pub(super) fn cancelled() -> Self {
        Self {
            code: -18,
            body: serde_json::to_string(&SimpleFailure { error: "cancelled" })
                .expect("cancellation body is serializable"),
        }
    }

    pub(super) fn job_failed(failure: &PrintTransferFailure) -> Self {
        Self {
            code: -19,
            body: serde_json::to_string(&TransferFailure {
                error: "job_failed",
                failure,
            })
            .expect("print transfer failure body is serializable"),
        }
    }

    fn field(error: &'static str, field: &'static str) -> Self {
        Self {
            code: -19,
            body: serde_json::to_string(&FieldFailure { error, field })
                .expect("field error body is serializable"),
        }
    }
}

pub(super) unsafe fn admit(raw: &PluginStudioPrintParams) -> Result<AdmittedPrint, PrintFailure> {
    unsafe {
        let hub_url = raw.snapshot.hub_url.read("dev_id")?;
        let hub_url = crate::normalize_hub_url(hub_url)
            .ok_or_else(|| PrintFailure::simple("invalid_hub_url"))?;
        let token = raw.snapshot.token.read("dev_id")?;
        if token.trim().is_empty() {
            return Err(PrintFailure::simple("invalid_auth_token"));
        }
        let dev_id = required(raw.dev_id.read("dev_id")?, "dev_id")?;
        let printer_id = required(raw.snapshot.printer_id.read("dev_id")?, "dev_id")?;
        if raw.snapshot.printer_authorized == 0 || raw.snapshot.account_transition_pending != 0 {
            return Err(PrintFailure::invalid("dev_id"));
        }
        drop(dev_id);

        drop(raw.ftp_folder.read("ftp_folder")?);
        drop(raw.dev_ip.read("dev_ip")?);
        drop(raw.username.read("username")?);
        drop(raw.password.read("password")?);
        let _ = raw.use_ssl_for_ftp;
        let _ = raw.use_ssl_for_mqtt;
        reject_non_default(&raw.ftp_file.read("ftp_file")?, "ftp_file")?;
        reject_non_default(&raw.ftp_file_md5.read("ftp_file_md5")?, "ftp_file_md5")?;
        reject_non_default(&raw.extra_options.read("extra_options")?, "extra_options")?;
        unsupported_non_default(&raw.dst_file.read("dst_file")?, "dst_file")?;
        if raw.task_ext_change_assist != 0 {
            return Err(PrintFailure::unsupported("task_ext_change_assist"));
        }

        let connection_type = match raw.connection_type.read("connection_type")?.as_str() {
            "" | "cloud" => "cloud".to_owned(),
            _ => return Err(PrintFailure::invalid("connection_type")),
        };
        let print_type = raw.print_type.read("print_type")?;
        if print_type.is_empty() {
            return Err(PrintFailure::invalid("print_type"));
        }
        if print_type != "from_normal" {
            return Err(PrintFailure::unsupported("print_type"));
        }
        let plate_index = u32::try_from(raw.plate_index)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| PrintFailure::invalid("plate_index"))?;
        let auto_bed_leveling = calibration(raw.auto_bed_leveling, "auto_bed_leveling")?;
        let auto_flow_cali = calibration(raw.auto_flow_cali, "auto_flow_cali")?;
        let auto_offset_cali = calibration(raw.auto_offset_cali, "auto_offset_cali")?;
        let extruder_cali_manual_mode = i8::try_from(raw.extruder_cali_manual_mode)
            .ok()
            .filter(|value| matches!(value, -1..=1))
            .ok_or_else(|| PrintFailure::invalid("extruder_cali_manual_mode"))?;

        let artifact = required(raw.filename.read("filename")?, "filename")?;
        let artifact_path = PathBuf::from(&artifact);
        let artifact_filename = basename(&artifact_path)
            .ok_or_else(|| PrintFailure::invalid("filename"))?
            .to_owned();
        let bed_type = raw.task_bed_type.read("task_bed_type")?;
        if !matches!(
            bed_type.as_str(),
            "supertack_plate" | "cool_plate" | "eng_plate" | "hot_plate" | "textured_plate"
        ) {
            return Err(PrintFailure::invalid("task_bed_type"));
        }

        Ok(AdmittedPrint {
            hub_url,
            token,
            printer_id,
            account_epoch: raw.snapshot.account_epoch,
            cache_generation: raw.snapshot.cache_generation,
            firmware_generation: raw.snapshot.firmware_generation,
            task_name: raw.task_name.read("task_name")?,
            project_name: raw.project_name.read("project_name")?,
            preset_name: raw.preset_name.read("preset_name")?,
            artifact_path,
            artifact_filename,
            config_filename: raw.config_filename.read("config_filename")?,
            config_plate_index: None,
            plate_index,
            nozzle_mapping: strict_json(
                &raw.nozzle_mapping.read("nozzle_mapping")?,
                "nozzle_mapping",
            )?,
            ams_mapping: strict_json(&raw.ams_mapping.read("ams_mapping")?, "ams_mapping")?,
            ams_mapping2: strict_json(&raw.ams_mapping2.read("ams_mapping2")?, "ams_mapping2")?,
            ams_mapping_info: strict_json(
                &raw.ams_mapping_info.read("ams_mapping_info")?,
                "ams_mapping_info",
            )?,
            nozzles_info: strict_json(&raw.nozzles_info.read("nozzles_info")?, "nozzles_info")?,
            connection_type,
            comments: raw.comments.read("comments")?,
            origin_profile_id: i64::from(raw.origin_profile_id),
            stl_design_id: i64::from(raw.stl_design_id),
            origin_model_id: raw.origin_model_id.read("origin_model_id")?,
            print_type,
            dev_name: raw.dev_name.read("dev_name")?,
            bed_leveling: raw.task_bed_leveling != 0,
            flow_cali: raw.task_flow_cali != 0,
            vibration_cali: raw.task_vibration_cali != 0,
            layer_inspect: raw.task_layer_inspect != 0,
            timelapse: raw.task_record_timelapse != 0,
            timelapse_use_internal: raw.task_timelapse_use_internal != 0,
            use_ams: raw.task_use_ams != 0,
            bed_type,
            auto_bed_leveling,
            auto_flow_cali,
            auto_offset_cali,
            extruder_cali_manual_mode,
            try_emmc_print: raw.try_emmc_print != 0,
            svc_context: raw.svc_context.read("svc_context")?,
            slicer_uid: raw.slicer_uid.read("slicer_uid")?,
        })
    }
}

impl AdmittedPrint {
    pub(super) unsafe fn matches_snapshot(&self, snapshot: &PluginStudioSnapshot) -> bool {
        unsafe {
            snapshot.printer_authorized != 0
                && snapshot.account_transition_pending == 0
                && snapshot.account_epoch == self.account_epoch
                && snapshot.cache_generation == self.cache_generation
                && snapshot.firmware_generation == self.firmware_generation
                && snapshot
                    .hub_url
                    .read("hub_url")
                    .ok()
                    .and_then(crate::normalize_hub_url)
                    .is_some_and(|value| value == self.hub_url)
                && snapshot
                    .token
                    .read("dev_id")
                    .is_ok_and(|value| value == self.token)
                && snapshot
                    .printer_id
                    .read("dev_id")
                    .is_ok_and(|value| value == self.printer_id)
        }
    }
}

pub(super) fn load_config_metadata(print: &mut AdmittedPrint) -> Result<(), PrintFailure> {
    if print.config_filename.is_empty() {
        return Ok(());
    }
    let plate_index =
        super::config::plate_index(Path::new(&print.config_filename)).map_err(|error| {
            eprintln!(
                "pandar network plugin config failed: stage=config_metadata category={}",
                super::config::diagnostic_category(&error)
            );
            PrintFailure::invalid("config_filename")
        })?;
    print.config_plate_index = Some(plate_index);
    print.config_filename.clear();
    Ok(())
}

fn strict_json<T>(value: &str, field: &'static str) -> Result<T, PrintFailure>
where
    T: DeserializeOwned + Default,
{
    if value.is_empty() {
        return Ok(T::default());
    }
    serde_json::from_str(value).map_err(|error| {
        diagnose_json(&error, &format!("parse Studio print parameter {field}"));
        PrintFailure::invalid(field)
    })
}

fn calibration(value: i32, field: &'static str) -> Result<PrintCalibrationMode, PrintFailure> {
    let value = u8::try_from(value).map_err(|_| PrintFailure::invalid(field))?;
    PrintCalibrationMode::try_from(value).map_err(|_| PrintFailure::invalid(field))
}

fn required(value: String, field: &'static str) -> Result<String, PrintFailure> {
    if value.trim().is_empty() {
        Err(PrintFailure::invalid(field))
    } else {
        Ok(value)
    }
}

fn reject_non_default(value: &str, field: &'static str) -> Result<(), PrintFailure> {
    if value.is_empty() {
        Ok(())
    } else {
        Err(PrintFailure::invalid(field))
    }
}

fn unsupported_non_default(value: &str, field: &'static str) -> Result<(), PrintFailure> {
    if value.is_empty() {
        Ok(())
    } else {
        Err(PrintFailure::unsupported(field))
    }
}

fn basename(path: &Path) -> Option<&str> {
    path.file_name().and_then(|value| value.to_str())
}
