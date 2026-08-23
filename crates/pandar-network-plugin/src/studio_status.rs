mod capabilities;
mod device;
mod firmware;
mod input;
mod list;
mod materials;
mod payload;
mod request;
mod scalar;

pub(super) use firmware::{
    acknowledgement_callback_json, current_firmware_json, firmware_refresh_failure_json,
    firmware_refresh_success_json, firmware_reset_json,
};
pub use list::{
    FirmwareProjection, PrinterObservation, StudioStatusProjection, project_hub_printers,
    project_stream_device,
};

pub(crate) use list::FirmwareObservation;

pub(super) fn local_connect_json(dev_id: &str, model: &str) -> String {
    payload::local_connect_json(dev_id, model)
}

pub(super) fn classify_status_request(message: &str) -> (i32, String) {
    request::classify_status_request(message)
}

pub(crate) use request::{StudioStatusRequest, parse_status_request};
