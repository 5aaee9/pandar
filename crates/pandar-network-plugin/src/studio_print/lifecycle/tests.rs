use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    path::PathBuf,
    time::Duration,
};

use pandar_core::PrintCalibrationMode;

use super::*;

struct FakePoller {
    now: Cell<Instant>,
    poll_elapsed: RefCell<VecDeque<Duration>>,
    poll_budgets: RefCell<Vec<Duration>>,
    sleeps: RefCell<Vec<Duration>>,
}

impl FakePoller {
    fn new(now: Instant, poll_elapsed: impl IntoIterator<Item = Duration>) -> Self {
        Self {
            now: Cell::new(now),
            poll_elapsed: RefCell::new(poll_elapsed.into_iter().collect()),
            poll_budgets: RefCell::new(Vec::new()),
            sleeps: RefCell::new(Vec::new()),
        }
    }
}

impl JobPoller for FakePoller {
    fn now(&self) -> Instant {
        self.now.get()
    }

    async fn poll(&self, submission_id: i32, deadline: Instant) -> Result<JobState, PrintFailure> {
        self.poll_budgets
            .borrow_mut()
            .push(deadline.saturating_duration_since(self.now.get()));
        let elapsed = self
            .poll_elapsed
            .borrow_mut()
            .pop_front()
            .expect("fake poll duration");
        self.now.set(self.now.get() + elapsed);
        Ok(JobState {
            studio_submission_id: submission_id,
            job_status: HubJobStatus::Queued,
            print_status: HubPrintStatus::Pending,
            failure: None,
        })
    }

    async fn sleep_until(&self, deadline: Instant) {
        self.sleeps
            .borrow_mut()
            .push(deadline.saturating_duration_since(self.now.get()));
        self.now.set(deadline);
    }
}

#[tokio::test]
async fn poll_requests_and_sleeps_share_one_absolute_deadline() {
    let started_at = Instant::now();
    let deadline = started_at + Duration::from_millis(250);
    let poller = FakePoller::new(
        started_at,
        [Duration::from_millis(40), Duration::from_millis(40)],
    );

    let failure = poll_until_complete(
        &poller,
        &transport::client().unwrap_or_else(|_| panic!("build test HTTP client")),
        &print(),
        callbacks(),
        42,
        deadline,
    )
    .await
    .unwrap_err();

    assert_eq!(failure.body, r#"{"error":"delivery_timeout"}"#);
    assert_eq!(
        *poller.poll_budgets.borrow(),
        [Duration::from_millis(250), Duration::from_millis(110)]
    );
    assert_eq!(
        *poller.sleeps.borrow(),
        [Duration::from_millis(100), Duration::from_millis(70)]
    );
}

#[tokio::test]
async fn poll_result_arriving_after_the_operation_deadline_is_rejected() {
    let started_at = Instant::now();
    let deadline = started_at + Duration::from_millis(25);
    let poller = FakePoller::new(started_at, [Duration::from_millis(26)]);

    let failure = poll_until_complete(
        &poller,
        &transport::client().unwrap_or_else(|_| panic!("build test HTTP client")),
        &print(),
        callbacks(),
        42,
        deadline,
    )
    .await
    .unwrap_err();

    assert_eq!(failure.body, r#"{"error":"delivery_timeout"}"#);
    assert_eq!(*poller.poll_budgets.borrow(), [Duration::from_millis(25)]);
    assert!(poller.sleeps.borrow().is_empty());
}

fn callbacks() -> PluginStudioCallbacks {
    PluginStudioCallbacks {
        context: std::ptr::null_mut(),
        update: None,
        cancelled: None,
        wait: None,
        snapshot: None,
    }
}

fn print() -> AdmittedPrint {
    AdmittedPrint {
        hub_url: "http://127.0.0.1".to_owned(),
        token: "token".to_owned(),
        printer_id: "printer".to_owned(),
        account_epoch: 1,
        cache_generation: 1,
        firmware_generation: 1,
        task_name: "task".to_owned(),
        project_name: "project".to_owned(),
        preset_name: "preset".to_owned(),
        artifact_path: PathBuf::from("unused.3mf"),
        artifact_filename: "artifact.3mf".to_owned(),
        config_filename: String::new(),
        config_plate_index: None,
        plate_index: 1,
        nozzle_mapping: Vec::new(),
        ams_mapping: Vec::new(),
        ams_mapping2: Vec::new(),
        ams_mapping_info: Vec::new(),
        nozzles_info: Vec::new(),
        connection_type: "cloud".to_owned(),
        comments: String::new(),
        origin_profile_id: 0,
        stl_design_id: 0,
        origin_model_id: String::new(),
        print_type: String::new(),
        dev_name: "printer".to_owned(),
        bed_leveling: false,
        flow_cali: false,
        vibration_cali: false,
        layer_inspect: false,
        timelapse: false,
        timelapse_use_internal: false,
        use_ams: false,
        bed_type: String::new(),
        auto_bed_leveling: PrintCalibrationMode::Off,
        auto_flow_cali: PrintCalibrationMode::Off,
        auto_offset_cali: PrintCalibrationMode::Off,
        extruder_cali_manual_mode: 0,
        try_emmc_print: false,
        svc_context: String::new(),
        slicer_uid: String::new(),
    }
}
