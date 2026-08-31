use std::ffi::c_void;

use reqwest::Method;
use serde::Deserialize;

use super::{
    admission::PrintFailure,
    ffi::{PluginBytes, PluginStudioAccount, account_from_ptr},
    tasks, transport,
};
use crate::PluginHttpResult;
pub(super) use crate::cancellation::{
    RequestCancellation as ModelTaskCancellation, RequestCancelled as StudioModelTaskCancelled,
};

#[repr(C)]
pub struct PluginStudioModelTask {
    pub job_id: i32,
    pub design_id: i32,
    pub profile_id: i32,
    pub instance_id: i32,
    pub task_id: PluginBytes,
    pub model_id: PluginBytes,
    pub model_name: PluginBytes,
    pub profile_name: PluginBytes,
}

pub type StudioModelTaskVisitor = extern "C" fn(*mut c_void, *const PluginStudioModelTask) -> i32;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelTaskResponse {
    job_id: i32,
    design_id: i32,
    profile_id: i32,
    instance_id: i32,
    task_id: String,
    model_id: String,
    model_name: String,
    profile_name: String,
}

impl ModelTaskResponse {
    fn valid_for(&self, requested: &str, requested_id: i32) -> bool {
        self.job_id == requested_id
            && self.task_id == requested
            && self.design_id == 0
            && self.profile_id == 0
            && self.instance_id == 0
            && self.model_id.is_empty()
            && !self.model_name.trim().is_empty()
            && !self.profile_name.trim().is_empty()
    }

    fn as_plugin_task(&self) -> PluginStudioModelTask {
        PluginStudioModelTask {
            job_id: self.job_id,
            design_id: self.design_id,
            profile_id: self.profile_id,
            instance_id: self.instance_id,
            task_id: bytes(&self.task_id),
            model_id: bytes(&self.model_id),
            model_name: bytes(&self.model_name),
            profile_name: bytes(&self.profile_name),
        }
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `account`, `task_id`, and nested byte views must remain valid for this call. Account snapshot and
/// visitor callbacks plus their contexts must remain callable for the full operation. Successful
/// snapshot callback byte views must stay readable until return, and the visitor must copy borrowed
/// task byte views before returning.
pub unsafe extern "C" fn pandar_plugin_studio_get_model_task(
    account: *const PluginStudioAccount,
    task_id: PluginBytes,
    visitor_context: *mut c_void,
    visitor: Option<StudioModelTaskVisitor>,
) -> PluginHttpResult {
    unsafe {
        get_model_task(
            account,
            task_id,
            visitor_context,
            visitor,
            ModelTaskCancellation::disabled(),
        )
    }
}

pub(super) unsafe fn get_model_task(
    account: *const PluginStudioAccount,
    task_id: PluginBytes,
    visitor_context: *mut c_void,
    visitor: Option<StudioModelTaskVisitor>,
    cancellation: ModelTaskCancellation,
) -> PluginHttpResult {
    let Some(visitor) = visitor else {
        return tasks::failure_result(400, "invalid_model_task_target");
    };
    if visitor_context.is_null() {
        return tasks::failure_result(400, "invalid_model_task_target");
    }
    let account = match unsafe { account_from_ptr(account) } {
        Ok(account) => account,
        Err(result) => return result,
    };
    let requested = match unsafe { task_id.read("task_id") } {
        Ok(task_id) => task_id,
        Err(_) => return tasks::failure_result(400, "invalid_task_id"),
    };
    let Some(requested_id) = canonical_task_id(&requested) else {
        return tasks::failure_result(400, "invalid_task_id");
    };

    crate::runtime().block_on(async move {
        let client = transport::client();
        let url = format!(
            "{}/api/v1/plugin/jobs/{requested}/model-task",
            account.hub_url
        );
        let reply = tokio::select! {
            biased;
            () = cancellation.wait() => Err(PrintFailure::simple("request_cancelled")),
            reply = transport::request(&client, Method::GET, url, &account.token) => reply,
        };
        if !account.freshness.current() {
            return tasks::failure_result(409, "stale_task_response");
        }
        deliver(reply, &requested, requested_id, visitor_context, visitor)
    })
}

fn deliver(
    reply: Result<transport::HttpReply, PrintFailure>,
    requested: &str,
    requested_id: i32,
    visitor_context: *mut c_void,
    visitor: StudioModelTaskVisitor,
) -> PluginHttpResult {
    match tasks::decoded_reply::<ModelTaskResponse>(reply) {
        Ok((status, task)) if task.valid_for(requested, requested_id) => {
            if visitor(visitor_context, &task.as_plugin_task()) == 1 {
                crate::result(0, status, "")
            } else {
                tasks::failure_result(502, "invalid_response")
            }
        }
        Ok((status, _)) => {
            tasks::failure_result(tasks::invalid_upstream_status(status), "invalid_response")
        }
        Err((status, failure)) => tasks::print_failure_result(status, failure),
    }
}

fn canonical_task_id(value: &str) -> Option<i32> {
    value
        .parse::<i32>()
        .ok()
        .filter(|id| *id > 0 && id.to_string() == value)
}

fn bytes(value: &str) -> PluginBytes {
    PluginBytes {
        ptr: value.as_ptr(),
        len: value.len(),
    }
}

#[cfg(test)]
mod tests;
