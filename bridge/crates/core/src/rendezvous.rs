//! Shared Bridge rendezvous policy: admission, remaining budget and safe errors.
use crate::platform::wifi::Job;
use std::time::{Duration, Instant};

pub const HTTP_HANDOFF_BUDGET: Duration = Duration::from_secs(30);
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

/// Finite identifiers only. Never store arbitrary transport/device messages.
pub fn wifi_error(error: &anyhow::Error) -> (&'static str, &'static str) {
    for cause in error.chain() {
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
