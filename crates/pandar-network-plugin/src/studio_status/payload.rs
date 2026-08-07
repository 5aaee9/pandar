use serde::Serialize;

use pandar_core::compatibility::studio_local_camera_supported;

use super::{device::StudioTelemetry, input::PrinterStatus};

#[derive(Serialize)]
struct PushStatusEnvelope {
    print: PushStatus,
}

#[derive(Serialize)]
struct PushStatus {
    command: &'static str,
    msg: u8,
    #[serde(flatten)]
    telemetry: StudioTelemetry,
    ipcam: CameraStatus,
    support_mqtt_alive: bool,
}

#[derive(Serialize)]
struct CameraStatus {
    ipcam_dev: &'static str,
    liveview: LiveView,
    rtsp_url: &'static str,
}

#[derive(Serialize)]
struct LiveView {
    local: &'static str,
    remote: &'static str,
}

pub(super) fn push_status_json(printer: &PrinterStatus, online: bool) -> String {
    let camera = printer.studio_local_camera
        && online
        && studio_local_camera_supported(printer.dev_model_name.as_deref());
    serde_json::to_string(&PushStatusEnvelope {
        print: PushStatus {
            command: "push_status",
            msg: 0,
            telemetry: StudioTelemetry::from(printer),
            ipcam: CameraStatus {
                ipcam_dev: if camera { "1" } else { "0" },
                liveview: LiveView {
                    local: "none",
                    remote: if camera { "tutk" } else { "none" },
                },
                rtsp_url: "",
            },
            support_mqtt_alive: online,
        },
    })
    .expect("Studio push status is serializable")
}

pub(super) fn local_connect_json(dev_id: &str, model: &str) -> String {
    serde_json::to_string(&LocalConnectStatus {
        dev_name: dev_id,
        dev_id,
        dev_ip: "",
        dev_type: model,
    })
    .expect("Studio local connect status is serializable")
}

#[derive(Serialize)]
struct LocalConnectStatus<'a> {
    dev_name: &'a str,
    dev_id: &'a str,
    dev_ip: &'a str,
    dev_type: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_camera_status_is_projected_only_for_verified_online_models() {
        for model in ["N1", "N2S", "C12", "N9"] {
            let printer = serde_json::from_value::<PrinterStatus>(serde_json::json!({
                "dev_model_name": model,
                "studio_local_camera": true
            }))
            .unwrap();
            let status =
                serde_json::from_str::<serde_json::Value>(&push_status_json(&printer, true))
                    .unwrap();

            assert_eq!(status["print"]["ipcam"]["ipcam_dev"], "1", "{model}");
            assert_eq!(status["print"]["ipcam"]["liveview"]["local"], "none");
            assert_eq!(status["print"]["ipcam"]["liveview"]["remote"], "tutk");
            assert_eq!(status["print"]["ipcam"]["rtsp_url"], "");
        }
    }

    #[test]
    fn local_camera_status_fails_closed_without_complete_evidence() {
        for (model, available, online) in [
            ("C11", true, true),
            ("BL-P001", true, true),
            ("Future Printer", true, true),
            ("N1", false, true),
            ("N1", true, false),
        ] {
            let printer = serde_json::from_value::<PrinterStatus>(serde_json::json!({
                "dev_model_name": model,
                "studio_local_camera": available
            }))
            .unwrap();
            let status =
                serde_json::from_str::<serde_json::Value>(&push_status_json(&printer, online))
                    .unwrap();

            assert_eq!(status["print"]["ipcam"]["ipcam_dev"], "0", "{model}");
            assert_eq!(status["print"]["ipcam"]["liveview"]["remote"], "none");
        }
    }
}
