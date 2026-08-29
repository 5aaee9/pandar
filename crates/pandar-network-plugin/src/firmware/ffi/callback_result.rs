use super::{CALLBACK_NONE, PluginFirmwareCallbackResult};
use crate::firmware::callbacks::{FirmwareTunnel, ReadyFirmwareCallback};

pub(super) fn callback_result(callback: ReadyFirmwareCallback) -> PluginFirmwareCallbackResult {
    let generation = callback.generation;
    let origin_tick = callback.origin_tick;
    let local_generation = callback.local_generation;
    let cache_generation = callback.cache_generation;
    let (dev_id_ptr, dev_id_len, dev_id_cap) = allocation(callback.dev_id);
    let (message_ptr, message_len, message_cap) = allocation(callback.message);
    PluginFirmwareCallbackResult {
        status: 0,
        generation,
        origin_tick,
        local_generation,
        cache_generation,
        dev_id_ptr,
        dev_id_len,
        dev_id_cap,
        message_ptr,
        message_len,
        message_cap,
        tunnel: match callback.tunnel {
            FirmwareTunnel::Cloud => 0,
            FirmwareTunnel::Local => 1,
        },
    }
}

pub(super) fn empty_callback() -> PluginFirmwareCallbackResult {
    PluginFirmwareCallbackResult {
        status: CALLBACK_NONE,
        generation: 0,
        origin_tick: 0,
        local_generation: 0,
        cache_generation: 0,
        dev_id_ptr: std::ptr::null_mut(),
        dev_id_len: 0,
        dev_id_cap: 0,
        message_ptr: std::ptr::null_mut(),
        message_len: 0,
        message_cap: 0,
        tunnel: 0,
    }
}

fn allocation(value: String) -> (*mut u8, usize, usize) {
    let mut bytes = value.into_bytes();
    let parts = (bytes.as_mut_ptr(), bytes.len(), bytes.capacity());
    std::mem::forget(bytes);
    parts
}
