use serde::Serialize;

use crate::{PluginHttpResult, result, stable_error_body};

pub const DISPOSITION_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug)]
#[repr(u32)]
enum Operation {
    InitLog = 1,
    SetCert = 2,
    UpdateCert = 3,
    InstallCert = 4,
    StartSubscribe = 5,
    StopSubscribe = 6,
    Consent = 7,
    LocalPrintWithRecord = 8,
    UserPresets = 9,
    RequestSettingId = 10,
    PutSetting = 11,
    GetSettingList = 12,
    GetSettingList2 = 13,
    DeleteSetting = 14,
    ExtraHttpHeader = 15,
    UserMessages = 16,
    UserTaskReport = 17,
    HmsSnapshot = 18,
    DesignStaffPick = 19,
    StartPublish = 20,
    ModelPublishUrl = 21,
    ModelMallHome = 22,
    ModelMallDetail = 23,
    PutModelRating = 24,
    OssConfig = 25,
    PutRatingPicture = 26,
    GetModelRating = 27,
    MakerWorldPreference = 28,
    MakerWorldForYou = 29,
    GetFilaments = 30,
    CreateFilament = 31,
    UpdateFilament = 32,
    DeleteFilament = 33,
    GetFilamentConfig = 34,
    TrackEnable = 35,
    TrackRemoveFiles = 36,
    TrackEvent = 37,
    TrackHeader = 38,
    TrackUpdateProperty = 39,
    TrackGetProperty = 40,
    SendGcodeToSdcard = 41,
    LocalPrint = 42,
    SdcardPrint = 43,
    EnableMultiMachine = 44,
    StartDiscovery = 45,
    PingBind = 46,
    BindDetect = 47,
    Bind = 48,
    Unbind = 49,
    BindTicket = 50,
    BindStatus = 51,
    ModifyPrinterName = 52,
    StudioInfoUnavailable = 53,
}

impl Operation {
    fn from_raw(raw: u32) -> Option<Self> {
        Some(match raw {
            1 => Self::InitLog,
            2 => Self::SetCert,
            3 => Self::UpdateCert,
            4 => Self::InstallCert,
            5 => Self::StartSubscribe,
            6 => Self::StopSubscribe,
            7 => Self::Consent,
            8 => Self::LocalPrintWithRecord,
            9 => Self::UserPresets,
            10 => Self::RequestSettingId,
            11 => Self::PutSetting,
            12 => Self::GetSettingList,
            13 => Self::GetSettingList2,
            14 => Self::DeleteSetting,
            15 => Self::ExtraHttpHeader,
            16 => Self::UserMessages,
            17 => Self::UserTaskReport,
            18 => Self::HmsSnapshot,
            19 => Self::DesignStaffPick,
            20 => Self::StartPublish,
            21 => Self::ModelPublishUrl,
            22 => Self::ModelMallHome,
            23 => Self::ModelMallDetail,
            24 => Self::PutModelRating,
            25 => Self::OssConfig,
            26 => Self::PutRatingPicture,
            27 => Self::GetModelRating,
            28 => Self::MakerWorldPreference,
            29 => Self::MakerWorldForYou,
            30 => Self::GetFilaments,
            31 => Self::CreateFilament,
            32 => Self::UpdateFilament,
            33 => Self::DeleteFilament,
            34 => Self::GetFilamentConfig,
            35 => Self::TrackEnable,
            36 => Self::TrackRemoveFiles,
            37 => Self::TrackEvent,
            38 => Self::TrackHeader,
            39 => Self::TrackUpdateProperty,
            40 => Self::TrackGetProperty,
            41 => Self::SendGcodeToSdcard,
            42 => Self::LocalPrint,
            43 => Self::SdcardPrint,
            44 => Self::EnableMultiMachine,
            45 => Self::StartDiscovery,
            46 => Self::PingBind,
            47 => Self::BindDetect,
            48 => Self::Bind,
            49 => Self::Unbind,
            50 => Self::BindTicket,
            51 => Self::BindStatus,
            52 => Self::ModifyPrinterName,
            53 => Self::StudioInfoUnavailable,
            _ => return None,
        })
    }

    fn error(self) -> &'static str {
        match self {
            Self::LocalPrintWithRecord => "unsupported_local_print_with_record",
            Self::InitLog => "unsupported_plugin_log_initialization",
            Self::SetCert | Self::UpdateCert | Self::InstallCert => {
                "unsupported_device_certificate"
            }
            Self::StartSubscribe | Self::StopSubscribe => "unsupported_subscription_control",
            Self::Consent => "unsupported_consent_persistence",
            Self::UserPresets
            | Self::RequestSettingId
            | Self::PutSetting
            | Self::GetSettingList
            | Self::GetSettingList2
            | Self::DeleteSetting => "unsupported_cloud_settings",
            Self::ExtraHttpHeader => "unsupported_extra_http_headers",
            Self::UserMessages => "unsupported_user_messages",
            Self::UserTaskReport => "unsupported_user_task_report",
            Self::HmsSnapshot => "unsupported_hms_snapshot",
            Self::DesignStaffPick
            | Self::StartPublish
            | Self::ModelPublishUrl
            | Self::ModelMallHome
            | Self::ModelMallDetail
            | Self::PutModelRating
            | Self::OssConfig
            | Self::PutRatingPicture
            | Self::GetModelRating
            | Self::MakerWorldPreference
            | Self::MakerWorldForYou => "unsupported_makerworld",
            Self::GetFilaments
            | Self::CreateFilament
            | Self::UpdateFilament
            | Self::DeleteFilament
            | Self::GetFilamentConfig => "unsupported_cloud_filaments",
            Self::SendGcodeToSdcard | Self::LocalPrint | Self::SdcardPrint => {
                "unsupported_file_transfer"
            }
            Self::EnableMultiMachine => "unsupported_multi_machine_mode",
            Self::StartDiscovery => "unsupported_direct_discovery",
            Self::PingBind
            | Self::BindDetect
            | Self::Bind
            | Self::Unbind
            | Self::BindTicket
            | Self::BindStatus
            | Self::ModifyPrinterName => "unsupported_direct_binding",
            Self::StudioInfoUnavailable => "studio_info_url_unconfigured",
            Self::TrackEnable
            | Self::TrackRemoveFiles
            | Self::TrackEvent
            | Self::TrackHeader
            | Self::TrackUpdateProperty
            | Self::TrackGetProperty => "never_track",
        }
    }

    fn is_tracking(self) -> bool {
        matches!(
            self,
            Self::TrackEnable
                | Self::TrackRemoveFiles
                | Self::TrackEvent
                | Self::TrackHeader
                | Self::TrackUpdateProperty
                | Self::TrackGetProperty
        )
    }

    fn abi_status(self) -> i32 {
        match self {
            Self::Bind => -5,
            operation if operation.is_tracking() => 0,
            _ => -19,
        }
    }
}

#[derive(Serialize)]
struct DispositionBody {
    disposition_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy: Option<&'static str>,
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_disposition(
    operation: u32,
    agent_valid: bool,
) -> PluginHttpResult {
    if !agent_valid {
        return result(-1, 0, stable_error_body("invalid_handle"));
    }
    let Some(operation) = Operation::from_raw(operation) else {
        return result(-19, 0, stable_error_body("unknown_studio_disposition"));
    };
    let tracking = operation.is_tracking();
    let body = serde_json::to_string(&DispositionBody {
        disposition_version: DISPOSITION_VERSION,
        error: (!tracking).then(|| operation.error()),
        policy: tracking.then_some("never_track"),
    })
    .expect("Studio disposition body is serializable");
    result(
        operation.abi_status(),
        if tracking { 200 } else { 501 },
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_version_one_disposition_is_total_and_non_success_except_never_track() {
        for raw in 1..=53 {
            let operation = Operation::from_raw(raw).expect("version one operation");
            let result = pandar_plugin_studio_disposition(raw, true);
            assert_eq!(result.status == 0, operation.is_tracking());
        }
        assert!(Operation::from_raw(54).is_none());
    }

    #[test]
    fn bind_uses_the_pinned_studio_bind_failure_code() {
        let result = pandar_plugin_studio_disposition(Operation::Bind as u32, true);
        assert_eq!(result.status, -5);
    }
}
