use std::{collections::BTreeMap, ffi::c_void, slice};

use super::{
    Account, cache, http,
    model::{self, PresetRequest},
};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PresetBytes {
    pub ptr: *const u8,
    pub len: usize,
}

impl PresetBytes {
    pub(super) fn read(self) -> anyhow::Result<String> {
        if self.len == 0 {
            return Ok(String::new());
        }
        anyhow::ensure!(!self.ptr.is_null(), "preset string pointer is null");
        Ok(std::str::from_utf8(unsafe { slice::from_raw_parts(self.ptr, self.len) })?.to_owned())
    }
}

#[repr(C)]
pub struct PresetEntry {
    pub key: PresetBytes,
    pub value: PresetBytes,
}

pub type EntryVisitor = extern "C" fn(*mut c_void, PresetBytes, PresetBytes) -> i32;
pub type CheckCallback = extern "C" fn(*mut c_void, *const PresetEntry, usize) -> i32;
pub type IntCallback = extern "C" fn(*mut c_void, i32) -> i32;

#[repr(C)]
pub struct PresetCallbacks {
    pub context: *mut c_void,
    pub check: Option<CheckCallback>,
    pub progress: Option<IntCallback>,
    pub cancel: Option<IntCallback>,
    pub current: Option<IntCallback>,
}

#[repr(C)]
pub struct PresetResult {
    pub status: i32,
    pub http_code: u32,
    pub updated_time: i64,
    pub code: i32,
    pub id_ptr: *mut u8,
    pub id_len: usize,
    pub id_cap: usize,
}

fn result(status: i32, http_code: u32, updated_time: i64, code: i32, id: String) -> PresetResult {
    let mut id = id.into_bytes();
    let output = PresetResult {
        status,
        http_code,
        updated_time,
        code,
        id_ptr: id.as_mut_ptr(),
        id_len: id.len(),
        id_cap: id.capacity(),
    };
    std::mem::forget(id);
    output
}

/// # Safety
/// `account` and `entries` must point to readable values for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pandar_plugin_personal_preset_mutate(
    operation: i32,
    account: *const Account,
    setting_id: PresetBytes,
    name: PresetBytes,
    entries: *const PresetEntry,
    entry_count: usize,
) -> PresetResult {
    let Some(account) = (unsafe { account.as_ref() }) else {
        return result(-1, 0, 0, 0, String::new());
    };
    let Ok(account) = Account::read(account) else {
        return result(operation_error(operation), 403, 0, 0, String::new());
    };
    if account.session_kind != 1
        || account.transition_pending != 0
        || account.user_id.is_empty()
        || account.token.is_empty()
    {
        return result(operation_error(operation), 403, 0, 0, String::new());
    }
    let output = (|| -> Result<(i64, String, i32), Failure> {
        let setting_id = setting_id.read()?;
        if operation != 1 && setting_id.is_empty() {
            return Err(Failure::Invalid);
        }
        if operation == 3 {
            http::delete(&account.hub_url, &account.token, &setting_id).map_err(Failure::Http)?;
            return Ok((0, String::new(), 0));
        }
        let request = PresetRequest::from_flat(name.read()?, read_entries(entries, entry_count)?)?;
        let mutation = match operation {
            1 => http::create(&account.hub_url, &account.token, &request),
            2 => http::update(&account.hub_url, &account.token, &setting_id, &request),
            _ => return Err(Failure::Invalid),
        }
        .map_err(Failure::Http)?;
        Ok((mutation.updated_time, mutation.setting_id, 0))
    })();
    match output {
        Ok((updated, id, code)) => result(
            0,
            if operation == 1 {
                201
            } else if operation == 2 {
                200
            } else {
                204
            },
            updated,
            code,
            id,
        ),
        Err(Failure::Http(error)) => {
            if let Some(cause) = error.cause {
                eprintln!("personal preset request failed: {cause:#}");
            } else {
                eprintln!(
                    "personal preset request rejected: status={} error={}",
                    error.status, error.error
                );
            }
            result(
                operation_error(operation),
                error.status,
                0,
                error.code.map_or(0, i32::from),
                String::new(),
            )
        }
        Err(Failure::Invalid | Failure::Cancelled | Failure::Stale) => {
            result(operation_error(operation), 400, 0, 0, String::new())
        }
    }
}

enum Failure {
    Http(http::HttpFailure),
    Invalid,
    Cancelled,
    Stale,
}
impl From<anyhow::Error> for Failure {
    fn from(_: anyhow::Error) -> Self {
        Self::Invalid
    }
}

/// # Safety
/// `account` must be readable and every supplied callback must remain valid during this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pandar_plugin_personal_preset_list(
    account: *const Account,
    bundle: PresetBytes,
    callbacks: PresetCallbacks,
) -> i32 {
    let Some(account) = (unsafe { account.as_ref() }) else {
        return -1;
    };
    let identity = account.identity;
    cache::reset(identity);
    let Ok(account) = Account::read(account) else {
        return -9;
    };
    if account.session_kind != 1
        || account.transition_pending != 0
        || account.user_id.is_empty()
        || account.token.is_empty()
    {
        return -9;
    }
    let generation = (account.account_epoch, account.config_epoch);
    let output = (|| -> Result<cache::PresetMap, Failure> {
        if callback(&callbacks, callbacks.current, 0) == 0 {
            return Err(Failure::Stale);
        }
        let bundle = bundle.read()?;
        let listed =
            http::list(&account.hub_url, &account.token, &bundle).map_err(Failure::Http)?;
        let total = listed.presets.len();
        if total > 1_000 {
            return Err(Failure::Invalid);
        }
        if listed.presets.iter().any(|preset| {
            preset.name.is_empty() || preset.name.contains('\0') || preset.setting_id.is_empty()
        }) {
            return Err(Failure::Invalid);
        }
        let mut names = std::collections::BTreeSet::new();
        if listed
            .presets
            .iter()
            .any(|preset| !names.insert(preset.name.clone()))
        {
            return Err(Failure::Invalid);
        }
        let mut temporary = cache::PresetMap::new();
        if total == 0 {
            callback(&callbacks, callbacks.progress, 100);
        }
        for (index, metadata) in listed.presets.iter().enumerate() {
            if callback(&callbacks, callbacks.cancel, 0) != 0 {
                return Err(Failure::Cancelled);
            }
            let check_map = BTreeMap::from([
                ("type".into(), metadata.preset_type.as_str().into()),
                ("name".into(), metadata.name.clone()),
                ("setting_id".into(), metadata.setting_id.clone()),
                ("updated_time".into(), metadata.updated_time.to_string()),
            ]);
            let want = callbacks.check.is_none() || call_check(&callbacks, &check_map) != 0;
            let values = if want {
                if callback(&callbacks, callbacks.cancel, 0) != 0 {
                    return Err(Failure::Cancelled);
                }
                let full = http::get(&account.hub_url, &account.token, &metadata.setting_id)
                    .map_err(Failure::Http)?;
                if full.setting_id != metadata.setting_id
                    || full.name != metadata.name
                    || full.preset_type != metadata.preset_type
                {
                    return Err(Failure::Invalid);
                }
                model::full_map(full, &account.user_id)
            } else {
                model::metadata_map(metadata, &account.user_id)
            };
            if values
                .iter()
                .map(|(key, value)| key.len() + value.len())
                .sum::<usize>()
                > model::MAX_MAP_BYTES
            {
                return Err(Failure::Invalid);
            }
            temporary.insert(metadata.name.clone(), values);
            callback(
                &callbacks,
                callbacks.progress,
                (((index + 1) * 100) / total) as i32,
            );
        }
        Ok(temporary)
    })();
    match output {
        Ok(presets) => {
            if callback(&callbacks, callbacks.current, 0) == 0 {
                cache::reset(account.identity);
                return -9;
            }
            cache::publish(account.identity, generation, presets);
            0
        }
        Err(Failure::Http(error)) => {
            cache::reset(account.identity);
            if let Some(cause) = error.cause {
                eprintln!("personal preset list failed: {cause:#}");
            } else {
                eprintln!(
                    "personal preset list rejected: status={} error={}",
                    error.status, error.error
                );
            }
            -9
        }
        Err(Failure::Cancelled) => {
            cache::reset(account.identity);
            -18
        }
        Err(Failure::Stale | Failure::Invalid) => {
            cache::reset(account.identity);
            -9
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_personal_preset_reset(identity: u64) {
    cache::reset(identity);
}

/// # Safety
/// `account`, `context`, and `visitor` must remain valid for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pandar_plugin_personal_preset_drain(
    account: *const Account,
    context: *mut c_void,
    visitor: Option<EntryVisitor>,
) -> i32 {
    let Some(account) = (unsafe { account.as_ref() }) else {
        return -1;
    };
    let identity = account.identity;
    let Ok(account) = Account::read(account) else {
        cache::reset(identity);
        return -19;
    };
    if account.session_kind != 1 || account.transition_pending != 0 {
        cache::reset(account.identity);
        return -19;
    }
    let Some(visitor) = visitor else {
        return -19;
    };
    for (name, values) in cache::drain(
        account.identity,
        (account.account_epoch, account.config_epoch),
    ) {
        for (key, value) in values {
            let composite = format!("{name}\0{key}");
            if visitor(context, bytes(&composite), bytes(&value)) != 0 {
                return -19;
            }
        }
    }
    0
}

fn read_entries(
    entries: *const PresetEntry,
    count: usize,
) -> anyhow::Result<BTreeMap<String, String>> {
    if count == 0 {
        return Ok(BTreeMap::new());
    }
    anyhow::ensure!(!entries.is_null(), "preset entries pointer is null");
    unsafe { slice::from_raw_parts(entries, count) }
        .iter()
        .map(|entry| Ok((entry.key.read()?, entry.value.read()?)))
        .collect()
}
fn bytes(value: &str) -> PresetBytes {
    PresetBytes {
        ptr: value.as_ptr(),
        len: value.len(),
    }
}
fn callback(callbacks: &PresetCallbacks, callback: Option<IntCallback>, value: i32) -> i32 {
    callback.map_or(0, |callback| callback(callbacks.context, value))
}
fn call_check(callbacks: &PresetCallbacks, map: &BTreeMap<String, String>) -> i32 {
    let entries: Vec<_> = map
        .iter()
        .map(|(key, value)| PresetEntry {
            key: bytes(key),
            value: bytes(value),
        })
        .collect();
    callbacks.check.map_or(1, |callback| {
        callback(callbacks.context, entries.as_ptr(), entries.len())
    })
}
fn operation_error(operation: i32) -> i32 {
    match operation {
        1 => -7,
        2 => -8,
        3 => -10,
        _ => -19,
    }
}
