use std::{ffi::c_void, slice};

use anyhow::{Context, ensure};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginAccountBytes {
    pub ptr: *const u8,
    pub len: usize,
}

impl PluginAccountBytes {
    pub(crate) fn from_str(value: &str) -> Self {
        Self {
            ptr: value.as_ptr(),
            len: value.len(),
        }
    }

    pub(crate) unsafe fn read(self, field: &'static str) -> anyhow::Result<String> {
        ensure!(
            !self.ptr.is_null() || self.len == 0,
            "{field} pointer is null"
        );
        let bytes = if self.len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(self.ptr, self.len) }
        };
        std::str::from_utf8(bytes)
            .with_context(|| format!("{field} is not UTF-8"))
            .map(ToOwned::to_owned)
    }
}

#[repr(C)]
pub struct PluginAccountView {
    pub config_dir: PluginAccountBytes,
    pub hub_url: PluginAccountBytes,
    pub token: PluginAccountBytes,
    pub user_id: PluginAccountBytes,
    pub user_name: PluginAccountBytes,
    pub avatar: PluginAccountBytes,
    pub profile_json: PluginAccountBytes,
    pub account_epoch: u64,
    pub config_epoch: u64,
    pub session_kind: i32,
    pub transition_pending: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginAccountNotification {
    Silent = 0,
    Logout = 2,
}

#[repr(C)]
pub struct PluginAccountMutation {
    pub action: i32,
    pub notification: PluginAccountNotification,
    pub hub_url: PluginAccountBytes,
    pub token: PluginAccountBytes,
    pub user_id: PluginAccountBytes,
    pub user_name: PluginAccountBytes,
    pub avatar: PluginAccountBytes,
    pub profile_json: PluginAccountBytes,
    pub session_kind: i32,
    pub error_body: PluginAccountBytes,
    pub http_code: u32,
}

pub type PluginAccountTransaction =
    unsafe extern "C" fn(*mut c_void, *const PluginAccountView, *mut PluginAccountMutation) -> i32;

pub type PluginWithCurrentAccount =
    unsafe extern "C" fn(*mut c_void, *mut c_void, Option<PluginAccountTransaction>) -> i32;

#[derive(Clone, Debug)]
pub(crate) struct AccountView {
    pub(crate) config_dir: String,
    pub(crate) hub_url: String,
    pub(crate) token: String,
    pub(crate) user_id: String,
    pub(crate) user_name: String,
    pub(crate) avatar: String,
    pub(crate) profile_json: String,
    pub(crate) account_epoch: u64,
    pub(crate) config_epoch: u64,
    pub(crate) session_kind: i32,
    pub(crate) transition_pending: bool,
}

impl AccountView {
    pub(crate) unsafe fn read(view: *const PluginAccountView) -> anyhow::Result<Self> {
        let view = unsafe { view.as_ref() }.context("account view is missing")?;
        Ok(Self {
            config_dir: unsafe { view.config_dir.read("account config directory") }?,
            hub_url: unsafe { view.hub_url.read("account Hub URL") }?,
            token: unsafe { view.token.read("account token") }?,
            user_id: unsafe { view.user_id.read("account user id") }?,
            user_name: unsafe { view.user_name.read("account user name") }?,
            avatar: unsafe { view.avatar.read("account avatar") }?,
            profile_json: unsafe { view.profile_json.read("account profile") }?,
            account_epoch: view.account_epoch,
            config_epoch: view.config_epoch,
            session_kind: view.session_kind,
            transition_pending: view.transition_pending != 0,
        })
    }
}

struct CaptureContext {
    view: Option<AccountView>,
    error: Option<anyhow::Error>,
}

unsafe extern "C" fn capture_transaction(
    context: *mut c_void,
    view: *const PluginAccountView,
    _: *mut PluginAccountMutation,
) -> i32 {
    let Some(context) = (unsafe { context.cast::<CaptureContext>().as_mut() }) else {
        return 1;
    };
    match unsafe { AccountView::read(view) } {
        Ok(view) => {
            context.view = Some(view);
            0
        }
        Err(error) => {
            context.error = Some(error);
            1
        }
    }
}

pub(crate) unsafe fn capture(
    context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
) -> anyhow::Result<AccountView> {
    let with_current = with_current.context("account transaction callback is missing")?;
    let mut capture = CaptureContext {
        view: None,
        error: None,
    };
    let status = unsafe {
        with_current(
            context,
            (&mut capture as *mut CaptureContext).cast(),
            Some(capture_transaction),
        )
    };
    if let Some(error) = capture.error {
        return Err(error);
    }
    ensure!(status == 0, "account transaction callback failed");
    capture.view.context("account transaction returned no view")
}

pub(super) unsafe fn transact(
    context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    transaction_context: *mut c_void,
    transaction: PluginAccountTransaction,
) -> anyhow::Result<()> {
    let with_current = with_current.context("account transaction callback is missing")?;
    let status = unsafe { with_current(context, transaction_context, Some(transaction)) };
    ensure!(status == 0, "account transaction callback failed");
    Ok(())
}
