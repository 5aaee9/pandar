use std::time::Duration;

pub(crate) const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
pub(crate) const DEFAULT_REPORT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunOutcome {
    ConnectedThenEnded,
}

#[derive(Debug)]
pub struct ReconnectBackoff {
    next: Duration,
}

impl ReconnectBackoff {
    pub fn new() -> Self {
        Self {
            next: Duration::from_secs(1),
        }
    }

    pub fn next_delay(&mut self) -> Duration {
        let delay = self.next;
        self.next = (self.next * 2).min(Duration::from_secs(30));
        delay
    }

    pub fn reset(&mut self) {
        self.next = Duration::from_secs(1);
    }
}

impl Default for ReconnectBackoff {
    fn default() -> Self {
        Self::new()
    }
}
