use serde::Serialize;

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
    serde_json::to_string(&PushStatusEnvelope {
        print: PushStatus {
            command: "push_status",
            msg: 0,
            telemetry: StudioTelemetry::from(printer),
            ipcam: CameraStatus {
                ipcam_dev: "0",
                liveview: LiveView {
                    local: "none",
                    remote: "none",
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
