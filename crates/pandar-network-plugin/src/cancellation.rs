use std::{ffi::c_void, future::pending, time::Duration};

pub(crate) type RequestCancelled = extern "C" fn(*mut c_void) -> i32;

#[derive(Clone, Copy)]
pub(crate) struct RequestCancellation {
    context: *mut c_void,
    cancelled: Option<RequestCancelled>,
}

impl RequestCancellation {
    pub(crate) fn disabled() -> Self {
        Self {
            context: std::ptr::null_mut(),
            cancelled: None,
        }
    }

    pub(crate) fn new(context: *mut c_void, cancelled: Option<RequestCancelled>) -> Self {
        Self { context, cancelled }
    }

    pub(crate) fn is_cancelled(self) -> bool {
        !self.context.is_null()
            && self
                .cancelled
                .is_some_and(|cancelled| cancelled(self.context) != 0)
    }

    pub(crate) async fn wait(self) {
        if self.context.is_null() || self.cancelled.is_none() {
            pending::<()>().await;
        }
        loop {
            if self.is_cancelled() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
