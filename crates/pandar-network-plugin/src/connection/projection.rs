use super::*;
use crate::studio_status::{FirmwareObservation, FirmwareProjection};

pub(super) struct CachedPrinterProjection {
    pub body: String,
    pub firmware: FirmwareProjection,
    pub printer_epoch: u64,
}

pub(super) fn cached_printer_projection(
    state: &ConnectionState,
) -> Option<CachedPrinterProjection> {
    if !state.printers_fresh {
        return None;
    }
    let observations = state
        .printers
        .values()
        .map(|printer| FirmwareObservation {
            dev_id: printer.dev_id.clone(),
            firmware: printer.firmware.clone(),
        })
        .collect();
    Some(CachedPrinterProjection {
        body: print_devices_envelope(&state.printers),
        firmware: FirmwareProjection::from_observations(state.printers.len(), observations),
        printer_epoch: state.printer_epoch,
    })
}
