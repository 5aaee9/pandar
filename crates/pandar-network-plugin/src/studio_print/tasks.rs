use reqwest::Method;
use serde::{Deserialize, Serialize};

use super::{
    admission::PrintFailure,
    diagnostics::diagnose_json,
    ffi::PluginStudioPlateResult,
    freshness::AccountFreshness,
    transport::{self, HttpReply},
};
use crate::PluginHttpResult;

pub(super) struct StudioAccount {
    pub(super) hub_url: String,
    pub(super) token: String,
    pub(super) freshness: AccountFreshness,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TaskPage {
    total: i32,
    hits: Vec<TaskHit>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TaskHit {
    id: i32,
    status: i32,
    #[serde(rename = "designId")]
    design_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(rename = "designTitle", skip_serializing_if = "Option::is_none")]
    design_title: Option<String>,
    #[serde(rename = "deviceName")]
    device_name: String,
    #[serde(rename = "deviceId")]
    device_id: String,
    cover: String,
    #[serde(rename = "startTime")]
    start_time: String,
    #[serde(rename = "endTime")]
    end_time: String,
    #[serde(rename = "profileId")]
    profile_id: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlateResponse {
    studio_submission_id: i32,
    plate_index: i32,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SubtaskResponse {
    content: String,
    context: SubtaskContext,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SubtaskContext {
    plates: Vec<SubtaskPlate>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SubtaskPlate {
    index: i32,
    thumbnail: Thumbnail,
    prediction: i64,
    weight: f64,
    filaments: Vec<SubtaskFilament>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Thumbnail {
    url: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SubtaskFilament {
    color: String,
    #[serde(rename = "type")]
    filament_type: String,
    used_g: String,
    used_m: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubtaskContent {
    info: SubtaskInfo,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubtaskInfo {
    plate_idx: i32,
}

pub(super) fn get_tasks(
    account: StudioAccount,
    dev_id: String,
    status: i32,
    offset: i32,
    limit: i32,
) -> PluginHttpResult {
    if offset < 0 || !(1..=100).contains(&limit) || !(0..=3).contains(&status) {
        return failure_result(400, "invalid_task_query");
    }
    crate::runtime().block_on(async move {
        let client = match transport::client() {
            Ok(client) => client,
            Err(failure) => return print_failure_result(0, failure),
        };
        let mut url = match reqwest::Url::parse(&format!("{}/api/v1/plugin/jobs", account.hub_url))
        {
            Ok(url) => url,
            Err(_) => return failure_result(400, "invalid_hub_url"),
        };
        url.query_pairs_mut()
            .append_pair("dev_id", &dev_id)
            .append_pair("status", &status.to_string())
            .append_pair("offset", &offset.to_string())
            .append_pair("limit", &limit.to_string());
        let reply = transport::request(&client, Method::GET, url.to_string(), &account.token).await;
        if !account.freshness.current() {
            return failure_result(409, "stale_task_response");
        }
        typed_response::<TaskPage>(reply, validate_task_page)
    })
}

pub(super) fn get_plate(account: StudioAccount, task_id: String) -> PluginStudioPlateResult {
    let Some(task_id) = valid_task_id(&task_id) else {
        return PluginStudioPlateResult {
            http: failure_result(400, "invalid_task_id"),
            plate_index: -1,
        };
    };
    crate::runtime().block_on(async move {
        let client = match transport::client() {
            Ok(client) => client,
            Err(failure) => {
                return PluginStudioPlateResult {
                    http: print_failure_result(0, failure),
                    plate_index: -1,
                };
            }
        };
        let reply = transport::request(
            &client,
            Method::GET,
            format!("{}/api/v1/plugin/jobs/{task_id}/plate", account.hub_url),
            &account.token,
        )
        .await;
        if !account.freshness.current() {
            return PluginStudioPlateResult {
                http: failure_result(409, "stale_task_response"),
                plate_index: -1,
            };
        }
        let (http, plate_index) = match decoded_reply::<PlateResponse>(reply) {
            Ok((status, plate))
                if plate.studio_submission_id == task_id && plate.plate_index > 0 =>
            {
                (crate::result(0, status, ""), plate.plate_index)
            }
            Ok((status, _)) => (
                failure_result(invalid_upstream_status(status), "invalid_response"),
                -1,
            ),
            Err((status, failure)) => (print_failure_result(status, failure), -1),
        };
        PluginStudioPlateResult { http, plate_index }
    })
}

pub(super) fn get_subtask(account: StudioAccount, task_id: String) -> PluginHttpResult {
    let Some(task_id) = valid_task_id(&task_id) else {
        return failure_result(400, "invalid_task_id");
    };
    crate::runtime().block_on(async move {
        let client = match transport::client() {
            Ok(client) => client,
            Err(failure) => return print_failure_result(0, failure),
        };
        let reply = transport::request(
            &client,
            Method::GET,
            format!("{}/api/v1/plugin/jobs/{task_id}/subtask", account.hub_url),
            &account.token,
        )
        .await;
        if !account.freshness.current() {
            return failure_result(409, "stale_task_response");
        }
        typed_response::<SubtaskResponse>(reply, validate_subtask)
    })
}

fn typed_response<T>(
    reply: Result<HttpReply, PrintFailure>,
    validate: impl FnOnce(&T) -> bool,
) -> PluginHttpResult
where
    T: for<'de> Deserialize<'de> + Serialize,
{
    match decoded_reply::<T>(reply) {
        Ok((status, value)) if validate(&value) => match serde_json::to_string(&value) {
            Ok(body) => crate::result(0, status, body),
            Err(error) => {
                diagnose_json(&error, "encode Studio task response");
                failure_result(invalid_upstream_status(status), "invalid_response")
            }
        },
        Ok((status, _)) => failure_result(invalid_upstream_status(status), "invalid_response"),
        Err((status, failure)) => print_failure_result(status, failure),
    }
}

pub(super) fn decoded_reply<T>(
    reply: Result<HttpReply, PrintFailure>,
) -> Result<(u32, T), (u32, PrintFailure)>
where
    T: for<'de> Deserialize<'de>,
{
    let reply = reply.map_err(|failure| (0, failure))?;
    let status = u32::from(reply.status);
    if !(200..300).contains(&reply.status) {
        return Err((status, transport::failure_from_reply(&reply)));
    }
    serde_json::from_str(&reply.body)
        .map(|value| (status, value))
        .map_err(|error| {
            diagnose_json(&error, "decode Studio task Hub response");
            (
                invalid_upstream_status(status),
                PrintFailure::simple("invalid_response"),
            )
        })
}

pub(super) fn invalid_upstream_status(status: u32) -> u32 {
    if (200..300).contains(&status) {
        502
    } else {
        status
    }
}

fn validate_task_page(page: &TaskPage) -> bool {
    page.total >= page.hits.len() as i32 && page.hits.iter().all(validate_task_hit)
}

fn validate_task_hit(hit: &TaskHit) -> bool {
    hit.id > 0
        && matches!(hit.status, 1..=3)
        && hit.design_id >= 0
        && hit.profile_id > 0
        && !hit.device_name.is_empty()
        && !hit.device_id.is_empty()
        && hit.cover.is_empty()
        && !hit.start_time.is_empty()
        && if hit.design_id > 0 {
            hit.title.is_none()
                && hit
                    .design_title
                    .as_ref()
                    .is_some_and(|title| !title.is_empty())
        } else {
            hit.design_title.is_none() && hit.title.as_ref().is_some_and(|title| !title.is_empty())
        }
}

fn validate_subtask(task: &SubtaskResponse) -> bool {
    let content = match serde_json::from_str::<SubtaskContent>(&task.content) {
        Ok(content) => content,
        Err(error) => {
            diagnose_json(&error, "decode Studio subtask content");
            return false;
        }
    };
    if content.info.plate_idx <= 0 {
        return false;
    }
    !task.context.plates.is_empty()
        && task.context.plates.iter().all(validate_subtask_plate)
        && task
            .context
            .plates
            .iter()
            .any(|plate| plate.index == content.info.plate_idx)
}

fn validate_subtask_plate(plate: &SubtaskPlate) -> bool {
    plate.index > 0
        && plate.thumbnail.url.is_empty()
        && (0..=i64::from(i32::MAX)).contains(&plate.prediction)
        && plate.weight.is_finite()
        && plate.weight >= 0.0
        && (plate.weight as f32).is_finite()
        && plate.filaments.iter().all(|filament| {
            !filament.color.is_empty()
                && !filament.filament_type.is_empty()
                && nonnegative_number(&filament.used_g)
                && nonnegative_number(&filament.used_m)
        })
}

fn nonnegative_number(value: &str) -> bool {
    value
        .parse::<f32>()
        .is_ok_and(|value| value.is_finite() && value >= 0.0)
}

fn valid_task_id(value: &str) -> Option<i32> {
    value.parse::<i32>().ok().filter(|value| *value > 0)
}

pub(super) fn failure_result(http_code: u32, error: &str) -> PluginHttpResult {
    crate::result(
        1,
        http_code,
        serde_json::to_string(&TaskError { error }).expect("task error is serializable"),
    )
}

pub(super) fn print_failure_result(http_code: u32, failure: PrintFailure) -> PluginHttpResult {
    crate::result(1, http_code, failure.body)
}

#[derive(Serialize)]
struct TaskError<'a> {
    error: &'a str,
}
