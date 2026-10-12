//! Shared Bridge rendezvous policy: admission, remaining budget and safe errors.
use crate::platform::wifi::Job;
use std::time::{Duration, Instant};

pub const HTTP_PROOF_BUDGET: Duration = Duration::from_secs(30);
pub const WIFI_WORK_BUDGET: Duration = Duration::from_secs(30);
pub const CLOSE_BUDGET: Duration = Duration::from_secs(2);

pub fn admit_wifi(job: &Job, now: u64) -> bool {
    job.runnable(now)
        && !matches!(
            job.state.as_str(),
            "waiting_input"
        )
}

/// Absolute monotonic deadline; no operation may restart the admitted window.
#[derive(Clone, Copy)]
pub struct Budget {
    until: Instant,
}
impl Budget {
    pub fn new(limit: Duration) -> Self {
        Self::at(Instant::now(), limit)
    }
    pub fn at(start: Instant, limit: Duration) -> Self {
        Self {
            until: start + limit,
        }
    }
    pub fn capped(self, limit: Duration) -> Self {
        Self {
            until: self.until.min(Instant::now() + limit),
        }
    }
    pub fn remaining(self) -> Duration {
        self.until.saturating_duration_since(Instant::now())
    }
}

/// A carrier rejected locally before emitting any bytes. Only this explicit
/// proof permits undoing the current attempt's conservative sent marker.
#[derive(Debug)]
pub struct NotDispatched;
impl std::fmt::Display for NotDispatched {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.write_str("wifi_window_budget")}
}
impl std::error::Error for NotDispatched {}

/// Transport proof, independent of whether the device applied the operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchOutcome { NotSent, WriteUnknown, AckUnknown }
#[derive(Debug)]
pub struct CommandFailure {
    pub outcome: DispatchOutcome,
    pub stage: &'static str,
    pub reason: &'static str,
}
impl CommandFailure {
    pub fn code(&self) -> &'static str {
        if self.outcome == DispatchOutcome::NotSent { "not_sent" } else { "result_unknown" }
    }
}
impl std::fmt::Display for CommandFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} ({})", self.code(), self.reason, self.stage)
    }
}
impl std::error::Error for CommandFailure {}

pub fn command_failure(error: &anyhow::Error) -> Option<&CommandFailure> {
    error.downcast_ref::<CommandFailure>()
        .or_else(||error.chain().find_map(|cause| cause.downcast_ref::<CommandFailure>()))
}
pub fn not_dispatched(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| cause.downcast_ref::<NotDispatched>().is_some()) ||
        command_failure(error).is_some_and(|e| e.outcome == DispatchOutcome::NotSent)
}

/// Finite identifiers only. Never store arbitrary transport/device messages.
pub fn wifi_error(error: &anyhow::Error) -> (&'static str, &'static str) {
    if let Some(failure) = command_failure(error) { return ("waiting_rendezvous", failure.code()); }
    for cause in error.chain() {
        if cause.downcast_ref::<NotDispatched>().is_some() { return ("waiting_rendezvous", "not_sent"); }
        if let Some(failure) = cause.downcast_ref::<CommandFailure>() {
            return ("waiting_rendezvous", failure.code());
        }
        if let Some(status) = cause.downcast_ref::<crate::device_client::HttpStatus>() {
            return match status.0 {
                401 | 403 => ("failed", "unauthorized"),
                400 | 404 | 413 => ("failed", "protocol_rejected"),
                _ => ("waiting_rendezvous", "http_unavailable"),
            };
        }
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            return if matches!(
                io.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) {
                ("waiting_rendezvous", "transport_timeout")
            } else {
                ("waiting_rendezvous", "transport_unavailable")
            };
        }
        if cause.downcast_ref::<serde_json::Error>().is_some() {
            return ("failed", "snapshot_shape");
        }
        let text = cause.to_string();
        match text.as_str() {
            "network_not_found" => return ("failed", "network_not_found"),
            "http_proof_budget_exhausted" | "sync_total_budget_exhausted" | "sync_no_progress" =>
                return ("waiting_rendezvous", "window_budget_exhausted"),
            "wifi_snapshot_timeout" | "wifi_window_budget" => {
                return ("waiting_rendezvous", "window_budget_exhausted")
            }
            "snapshot_expired" => return ("waiting_rendezvous", "snapshot_expired"),
            "physical_required" => return ("waiting_authorization", "physical_required"),
            "revision_conflict" => return ("conflict", "revision_conflict"),
            "occupied" => return ("conflict", "occupied"),
            "unauthorized" | "wifi_http_401" | "wifi_http_403" => {
                return ("failed", "unauthorized")
            }
            "unsupported" => return ("failed", "unsupported"),
            "wifi_mac_mismatch" | "wifi_snapshot_mismatch" | "wifi_snapshot_incomplete" => {
                return ("failed", "snapshot_mismatch")
            }
            "invalid_wifi_snapshot"
            | "invalid_wifi_network"
            | "wifi_snapshot_shape"
            | "wifi_snapshot_id"
            | "wifi_revision"
            | "wifi_snapshot_length"
            | "wifi_snapshot_offset"
            | "wifi_snapshot_chunk" => return ("failed", "snapshot_shape"),
            "wifi_snapshot_rejected" => return ("failed", "snapshot_rejected"),
            _ => {}
        }
    }
    ("waiting_rendezvous", "transport_unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sampled_deadline_cannot_restart_after_handoff_or_close() {
        let sampled = Instant::now() - Duration::from_secs(3);
        let legal = Budget::at(sampled, Duration::from_secs(3));
        assert!(legal.remaining().is_zero());
        assert!(legal.capped(WIFI_WORK_BUDGET).remaining().is_zero());
    }
    #[test]
    fn snapshot_errors_are_distinct_and_do_not_expose_input() {
        for (message, state, code) in [
            ("snapshot_expired", "waiting_rendezvous", "snapshot_expired"),
            ("network_not_found", "failed", "network_not_found"),
            ("http_proof_budget_exhausted", "waiting_rendezvous", "window_budget_exhausted"),
            ("wifi_snapshot_mismatch", "failed", "snapshot_mismatch"),
            ("invalid_wifi_snapshot", "failed", "snapshot_shape"),
            (
                "wifi_window_budget",
                "waiting_rendezvous",
                "window_budget_exhausted",
            ),
            ("wifi_http_401", "failed", "unauthorized"),
            (
                "secret password arbitrary input",
                "waiting_rendezvous",
                "transport_unavailable",
            ),
        ] {
            assert_eq!(wifi_error(&anyhow::anyhow!(message)), (state, code));
        }
    }
}
