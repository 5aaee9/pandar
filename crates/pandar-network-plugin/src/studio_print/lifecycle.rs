use std::time::Duration;

use pandar_core::PrintTransferFailure;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use super::{
    admission::{AdmittedPrint, PrintFailure},
    diagnostics::diagnose_json,
    ffi::PluginStudioCallbacks,
    transport::{self, HttpReply},
};

const OPERATION_TIMEOUT: Duration = Duration::from_secs(60);
const JOB_POLL_INTERVAL: Duration = Duration::from_millis(100);

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
    failure: Option<PrintTransferFailure>,
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
    let deadline = Instant::now() + OPERATION_TIMEOUT;
    let result = crate::runtime().block_on(async {
        match tokio::time::timeout_at(deadline, run(print, callbacks, deadline)).await {
            Ok(result) => result,
            Err(error) => Err(operation_deadline_failure(error)),
        }
    });
    match result {
        Ok(()) => 0,
        Err(failure) => {
            callbacks.error(&failure);
            failure.code
        }
    }
}

async fn run(
    print: AdmittedPrint,
    callbacks: PluginStudioCallbacks,
    deadline: Instant,
) -> Result<(), PrintFailure> {
    let client = transport::client()?;
    let created_reply = transport::submit(&client, &print, callbacks, deadline).await?;
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
        return retain_or_cancel_stale(&client, &print, submission_id, deadline).await;
    }

    let poller = HttpJobPoller {
        client: &client,
        print: &print,
    };
    poll_until_complete(&poller, &client, &print, callbacks, submission_id, deadline).await
}

trait JobPoller {
    fn now(&self) -> Instant;

    async fn poll(&self, submission_id: i32, deadline: Instant) -> Result<JobState, PrintFailure>;

    async fn sleep_until(&self, deadline: Instant);
}

struct HttpJobPoller<'a> {
    client: &'a reqwest::Client,
    print: &'a AdmittedPrint,
}

impl JobPoller for HttpJobPoller<'_> {
    fn now(&self) -> Instant {
        Instant::now()
    }

    async fn poll(&self, submission_id: i32, deadline: Instant) -> Result<JobState, PrintFailure> {
        poll(self.client, self.print, submission_id, deadline).await
    }

    async fn sleep_until(&self, deadline: Instant) {
        tokio::time::sleep_until(deadline).await;
    }
}

async fn poll_until_complete<P: JobPoller>(
    poller: &P,
    client: &reqwest::Client,
    print: &AdmittedPrint,
    callbacks: PluginStudioCallbacks,
    submission_id: i32,
    deadline: Instant,
) -> Result<(), PrintFailure> {
    let mut sending_emitted = false;
    loop {
        if poller.now() >= deadline {
            return Err(PrintFailure::simple("delivery_timeout"));
        }
        if !callbacks.snapshot_current(print) {
            return retain_or_cancel_stale(client, print, submission_id, deadline).await;
        }
        if callbacks.cancelled() {
            return cancel_or_retain_if_stale(client, print, callbacks, submission_id, deadline)
                .await;
        }

        let state = poller.poll(submission_id, deadline).await?;
        if poller.now() >= deadline {
            return Err(PrintFailure::simple("delivery_timeout"));
        }
        if !callbacks.snapshot_current(print) {
            return retain_or_cancel_stale(client, print, submission_id, deadline).await;
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
                return finish(client, print, callbacks, submission_id, deadline).await;
            }
            HubJobStatus::Failed => {
                return Err(match &state.failure {
                    Some(failure) => PrintFailure::job_failed(failure),
                    None => PrintFailure::simple("job_failed"),
                });
            }
            HubJobStatus::Cancelled => return Err(PrintFailure::simple("invalid_response")),
        }

        let sleep_deadline = (poller.now() + JOB_POLL_INTERVAL).min(deadline);
        poller.sleep_until(sleep_deadline).await;
    }
}

async fn retain_or_cancel_stale(
    client: &reqwest::Client,
    print: &AdmittedPrint,
    submission_id: i32,
    deadline: Instant,
) -> Result<(), PrintFailure> {
    match cancel(client, print, submission_id, deadline).await {
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
    deadline: Instant,
) -> Result<(), PrintFailure> {
    match cancel(client, print, submission_id, deadline).await {
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
    deadline: Instant,
) -> Result<(), PrintFailure> {
    callbacks.update(5, 0, "");
    if callbacks.cancelled() {
        return cancel_or_retain_if_stale(client, print, callbacks, submission_id, deadline).await;
    }
    let wait_info = serde_json::to_string(&WaitInfo {
        job_id: submission_id,
    })
    .expect("wait information is serializable");
    let waited = callbacks.wait(&wait_info);
    if callbacks.cancelled() {
        return cancel_or_retain_if_stale(client, print, callbacks, submission_id, deadline).await;
    }
    if Instant::now() >= deadline {
        return Err(PrintFailure::simple("delivery_timeout"));
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
    deadline: Instant,
) -> Result<JobState, PrintFailure> {
    let reply = transport::request_before(
        client,
        Method::GET,
        format!("{}/api/v1/plugin/jobs/{submission_id}", print.hub_url),
        &print.token,
        deadline,
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
    deadline: Instant,
) -> Result<(), PrintFailure> {
    let reply = transport::request_before(
        client,
        Method::POST,
        format!(
            "{}/api/v1/plugin/jobs/{submission_id}/cancel",
            print.hub_url
        ),
        &print.token,
        deadline,
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

fn operation_deadline_failure(error: tokio::time::error::Elapsed) -> PrintFailure {
    let error = anyhow::Error::new(error).context("complete synchronous Studio print operation");
    eprintln!("pandar network plugin print timed out: {error:#}");
    PrintFailure::simple("delivery_timeout")
}

#[cfg(test)]
mod tests;
