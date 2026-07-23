use reqwest::Method;
use serde::{Deserialize, Serialize};

use super::{
    admission::{AdmittedPrint, PrintFailure},
    diagnostics::diagnose_json,
    ffi::PluginStudioCallbacks,
    transport::{self, HttpReply},
};

const MAX_JOB_POLLS: usize = 600;

#[derive(Deserialize)]
struct CreatedPrint {
    task_id: i32,
    studio_submission_id: i32,
    status: String,
}

#[derive(Deserialize)]
struct JobState {
    studio_submission_id: i32,
    job_status: HubJobStatus,
    print_status: HubPrintStatus,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelState {
    studio_submission_id: i32,
    job_status: HubJobStatus,
    print_status: HubPrintStatus,
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum HubJobStatus {
    Queued,
    Sent,
    Acknowledged,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum HubPrintStatus {
    Pending,
    Stalled,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Serialize)]
struct WaitInfo {
    job_id: i32,
}

pub(super) fn start(print: AdmittedPrint, callbacks: PluginStudioCallbacks) -> i32 {
    if !callbacks.snapshot_current(&print) {
        let failure = PrintFailure::simple("stale_print_submission");
        callbacks.error(&failure);
        return failure.code;
    }
    if callbacks.cancelled() {
        let failure = PrintFailure::cancelled();
        callbacks.error(&failure);
        return failure.code;
    }
    callbacks.update(0, 0, "");
    match crate::runtime().block_on(run(print, callbacks)) {
        Ok(()) => 0,
        Err(failure) => {
            callbacks.error(&failure);
            failure.code
        }
    }
}

async fn run(print: AdmittedPrint, callbacks: PluginStudioCallbacks) -> Result<(), PrintFailure> {
    let client = transport::client()?;
    let created_reply = transport::submit(&client, &print, callbacks).await?;
    if created_reply.status != 201 {
        return Err(transport::failure_from_reply(&created_reply));
    }
    let created: CreatedPrint = decode(&created_reply)?;
    if created.task_id <= 0
        || created.task_id != created.studio_submission_id
        || created.status != "queued"
    {
        return Err(PrintFailure::simple("invalid_response"));
    }
    let submission_id = created.studio_submission_id;
    callbacks.update(2, 0, "");

    if !callbacks.snapshot_current(&print) {
        return retain_or_cancel_stale(&client, &print, submission_id).await;
    }

    let mut sending_emitted = false;
    for attempt in 0..MAX_JOB_POLLS {
        if !callbacks.snapshot_current(&print) {
            return retain_or_cancel_stale(&client, &print, submission_id).await;
        }
        if callbacks.cancelled() {
            return cancel_or_retain_if_stale(&client, &print, callbacks, submission_id).await;
        }

        let state = poll(&client, &print, submission_id).await?;
        if !callbacks.snapshot_current(&print) {
            return retain_or_cancel_stale(&client, &print, submission_id).await;
        }
        if state.studio_submission_id != submission_id {
            return Err(PrintFailure::simple("invalid_response"));
        }
        if state.job_status == HubJobStatus::Cancelled
            && state.print_status == HubPrintStatus::Cancelled
        {
            return Err(PrintFailure::cancelled());
        }
        match state.job_status {
            HubJobStatus::Queued | HubJobStatus::Sent => {}
            HubJobStatus::Acknowledged => {
                if !sending_emitted {
                    callbacks.update(3, 0, "");
                    sending_emitted = true;
                }
            }
            HubJobStatus::Succeeded => {
                if !sending_emitted {
                    callbacks.update(3, 0, "");
                }
                callbacks.update(4, 0, "");
                return finish(&client, &print, callbacks, submission_id).await;
            }
            HubJobStatus::Failed => return Err(PrintFailure::simple("job_failed")),
            HubJobStatus::Cancelled => return Err(PrintFailure::simple("invalid_response")),
        }
        if attempt + 1 < MAX_JOB_POLLS {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    Err(PrintFailure::simple("delivery_timeout"))
}

async fn retain_or_cancel_stale(
    client: &reqwest::Client,
    print: &AdmittedPrint,
    submission_id: i32,
) -> Result<(), PrintFailure> {
    match cancel(client, print, submission_id).await {
        Ok(()) => Err(PrintFailure::simple("stale_print_submission")),
        Err(failure) => {
            log_retained_submission(&failure);
            Ok(())
        }
    }
}

async fn cancel_or_retain_if_stale(
    client: &reqwest::Client,
    print: &AdmittedPrint,
    callbacks: PluginStudioCallbacks,
    submission_id: i32,
) -> Result<(), PrintFailure> {
    match cancel(client, print, submission_id).await {
        Ok(()) => Err(PrintFailure::cancelled()),
        Err(failure) if !callbacks.snapshot_current(print) => {
            log_retained_submission(&failure);
            Ok(())
        }
        Err(failure) => Err(failure),
    }
}

fn log_retained_submission(failure: &PrintFailure) {
    eprintln!(
        "pandar network plugin retained accepted Studio submission after unconfirmed stale cancellation: code={} body={}",
        failure.code, failure.body
    );
}

async fn finish(
    client: &reqwest::Client,
    print: &AdmittedPrint,
    callbacks: PluginStudioCallbacks,
    submission_id: i32,
) -> Result<(), PrintFailure> {
    callbacks.update(5, 0, "");
    if callbacks.cancelled() {
        return cancel_or_retain_if_stale(client, print, callbacks, submission_id).await;
    }
    let wait_info = serde_json::to_string(&WaitInfo {
        job_id: submission_id,
    })
    .expect("wait information is serializable");
    let waited = callbacks.wait(&wait_info);
    if callbacks.cancelled() {
        return cancel_or_retain_if_stale(client, print, callbacks, submission_id).await;
    }
    if !waited {
        return Err(PrintFailure::simple("wait_failed"));
    }
    callbacks.update(6, 0, "3");
    Ok(())
}

async fn poll(
    client: &reqwest::Client,
    print: &AdmittedPrint,
    submission_id: i32,
) -> Result<JobState, PrintFailure> {
    let reply = transport::request(
        client,
        Method::GET,
        format!("{}/api/v1/plugin/jobs/{submission_id}", print.hub_url),
        &print.token,
    )
    .await?;
    if !(200..300).contains(&reply.status) {
        return Err(transport::failure_from_reply(&reply));
    }
    decode(&reply)
}

async fn cancel(
    client: &reqwest::Client,
    print: &AdmittedPrint,
    submission_id: i32,
) -> Result<(), PrintFailure> {
    let reply = transport::request(
        client,
        Method::POST,
        format!(
            "{}/api/v1/plugin/jobs/{submission_id}/cancel",
            print.hub_url
        ),
        &print.token,
    )
    .await?;
    if !(200..300).contains(&reply.status) {
        return Err(transport::failure_from_reply(&reply));
    }
    let state: CancelState = decode(&reply)?;
    if state.studio_submission_id == submission_id
        && state.job_status == HubJobStatus::Cancelled
        && state.print_status == HubPrintStatus::Cancelled
    {
        Ok(())
    } else {
        Err(PrintFailure::simple("invalid_response"))
    }
}

fn decode<T: serde::de::DeserializeOwned>(reply: &HttpReply) -> Result<T, PrintFailure> {
    serde_json::from_str(&reply.body).map_err(|error| {
        diagnose_json(&error, "decode Studio print Hub response");
        PrintFailure::simple("invalid_response")
    })
}
