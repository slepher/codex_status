//! Per-device serial coordinator (v2 §2/§6/§7/§10): one active context, one
//! PublishJob, merged latest snapshots, push/full fingerprints, `data_seq`,
//! `full_sync_deadline` and PowerPlan state.
//!
//! Serial by design: publish, activate, data commit and OTA are mutually
//! exclusive for one device; different devices are independent.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::datasource::{assemble_fields, fingerprint};
use crate::platform::model::FieldTrigger;
use crate::platform::model::{
    Bundle, DataSnapshot, DeviceCapabilities, FieldRequirement, PlanMode, PowerPlan, PublishJob,
    PublishState, Quality, SourceSnapshot,
};

/// Small Data that must fit the current BLE budget, otherwise the bridge asks
/// for a Wi-Fi light session.
pub const BLE_DATA_BUDGET_BYTES: usize = 1800;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextState {
    pub context_id: String,
    pub created_at: u64,
    pub reason: String,
    pub next_seq: u64,
    pub acked_push_fp: Option<u64>,
    pub acked_full_fp: Option<u64>,
    pub full_sync_deadline: u64,
    pub last_applied_seq: u64,
    pub last_content_crc: Option<String>,
}

/// A delivery pinned when first attempted; retries reuse exactly this content so
/// a duplicate seq is idempotent and a stale ACK cannot confirm newer data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InFlight {
    pub kind: DeliveryKind,
    pub seq: u64,
    pub content_crc: String,
    pub payload: DataSnapshot,
    /// Fingerprints of the source snapshot at pin time: only these may be
    /// confirmed by the ACK, so newer data is never falsely acknowledged.
    pub push_fp: u64,
    pub full_fp: u64,
    pub sent_at: u64,
    pub attempts: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryKind {
    BleData,
    LightData,
    Bundle,
    Activate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Delivery {
    Idle,
    /// Device is unreachable; keep the intent, wait for a rendezvous opportunity.
    WaitingForRendezvous {
        reason: String,
    },
    Noop {
        time_sync: bool,
    },
    /// Complete small snapshot delivered over the rendezvous link.
    BleData(DataSnapshot),
    /// Complete snapshot delivered in a Wi-Fi light session.
    LightData(DataSnapshot),
    Bundle {
        job_id: String,
    },
    Activate {
        template_id: String,
        context_id: String,
    },
    /// New formal plan that must be sent (not a data packet).
    Plan(PowerPlan),
}

impl Delivery {
    pub fn kind(&self) -> Option<DeliveryKind> {
        match self {
            Delivery::BleData(_) => Some(DeliveryKind::BleData),
            Delivery::LightData(_) => Some(DeliveryKind::LightData),
            Delivery::Bundle { .. } => Some(DeliveryKind::Bundle),
            Delivery::Activate { .. } => Some(DeliveryKind::Activate),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ObservedPower {
    pub mode: Option<PlanMode>,
    pub remaining_s: u32,
    pub provisional: bool,
    #[serde(default)]
    pub provisional_remaining_s: u32,
    pub plan_id: u64,
    pub battery_percent: i32,
}

/// Bridge-side plan state: monotonic `plan_id`, last accepted result, and the
/// current light deadline (device monotonic clock is authoritative on-device;
/// this mirror is for the UI and for idempotent retries).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanState {
    pub next_plan_id: u64,
    pub last_sent: Option<PowerPlan>,
    #[serde(default)]
    pub last_sent_at: u64,
    pub last_accepted_id: u64,
    pub last_accepted_remaining_s: u32,
    pub last_accepted_at: u64,
    #[serde(default)]
    pub pending_explicit_light: Option<PowerPlan>,
    #[serde(default)]
    pub last_explicit_light_ack: Option<ExplicitLightAck>,
    #[serde(default)]
    pub light_hold_until: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplicitLightAck {
    pub plan_id: u64,
    pub accepted_remaining_s: u32,
    pub accepted_at: u64,
}

/// Bridge-side view of the device session (mirrors authenticated Status).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceSession {
    pub active_context_id: Option<String>,
    pub active_template_id: Option<String>,
    pub committed_job_id: Option<String>,
    pub data_seq: Option<u64>,
    pub applied_seq: Option<u64>,
    pub display_state: Option<String>,
    pub power: ObservedPower,
    pub last_status_at: Option<u64>,
    pub battery_percent: i32,
}

/// Bridge-side data snapshot state: latest complete snapshot, push/full
/// fingerprints, last confirmed fingerprints, full-sync deadline, next data_seq.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataState {
    pub push_fp: Option<u64>,
    pub full_fp: Option<u64>,
    pub acked_push_fp: Option<u64>,
    pub acked_full_fp: Option<u64>,
    pub push_dirty: bool,
    pub in_flight: Option<InFlight>,
}

impl Default for DataState {
    fn default() -> Self {
        Self {
            push_fp: None,
            full_fp: None,
            acked_push_fp: None,
            acked_full_fp: None,
            push_dirty: false,
            in_flight: None,
        }
    }
}

pub struct Coordinator {
    pub mac: String,
    pub caps: DeviceCapabilities,
    pub sources: BTreeMap<String, SourceSnapshot>,
    /// Requirement field -> trigger, resolved through the profile bindings.
    pub triggers: BTreeMap<String, FieldTrigger>,
    pub requirements: Vec<FieldRequirement>,
    pub active_template_id: Option<String>,
    pub context: Option<ContextState>,
    pub data: DataState,
    pub plan: PlanState,
    pub job: Option<PublishJob>,
    pub pending_activate: Option<String>,
    pub full_sync_s: u64,
    /// Data sync permission: only an explicit publish/UI/MCP action enables it.
    /// With sync off the coordinator caches snapshots but never delivers data.
    pub sync_enabled: bool,
    pub session: DeviceSession,
    /// Observed plan-id high water mark accepted by the device.
    pub device_plan_high: u64,
    pub last_sequence_reason: Option<String>,
}

pub const DEFAULT_FULL_SYNC_S: u64 = 3600;
pub const MAX_LIGHT_S: u32 = 600;
pub const RENDEZVOUS_S: u32 = 60;
pub const BOOT_PROVISIONAL_S: u32 = 300;

impl Coordinator {
    pub fn new(mac: &str, caps: DeviceCapabilities) -> Self {
        Self {
            mac: mac.to_uppercase(),
            caps,
            sources: BTreeMap::new(),
            triggers: BTreeMap::new(),
            requirements: Vec::new(),
            active_template_id: None,
            context: None,
            data: DataState::default(),
            plan: PlanState::default(),
            job: None,
            pending_activate: None,
            full_sync_s: DEFAULT_FULL_SYNC_S,
            sync_enabled: false,
            session: DeviceSession::default(),
            device_plan_high: 0,
            last_sequence_reason: None,
        }
    }

    /// Raise the bridge-side light hold (never shortens an existing hold).
    pub fn hold_light(&mut self, until: u64) {
        self.plan.light_hold_until = self.plan.light_hold_until.max(until);
    }

    /// Queue one explicit light action. Repeated requests before ACK reuse the
    /// exact plan so HTTP and BLE delivery cannot create competing IDs.
    pub fn queue_explicit_light(&mut self, now: u64, light_s: u32) -> PowerPlan {
        if let Some(plan) = &self.plan.pending_explicit_light {
            return plan.clone();
        }
        let duration = light_s.clamp(30, MAX_LIGHT_S.min(self.caps.max_light_s));
        let id = self.plan.next_plan_id.max(self.device_plan_high)
            .max(self.plan.last_sent.as_ref().map(|p| p.plan_id).unwrap_or(0)) + 1;
        let plan = PowerPlan::light(id, duration, RENDEZVOUS_S, "explicit");
        self.plan.next_plan_id = id;
        self.plan.last_sent = Some(plan.clone());
        self.plan.last_sent_at = now;
        self.plan.pending_explicit_light = Some(plan.clone());
        self.plan.last_explicit_light_ack = None;
        plan
    }

    pub fn cancel_explicit_light(&mut self) {
        self.plan.pending_explicit_light = None;
        self.plan.light_hold_until = 0;
    }

    /// Rebuild the active requirement/trigger contract (context switch, profile
    /// change, bundle install). Local device fields are excluded from remote
    /// fingerprints.
    pub fn set_contract(
        &mut self,
        active_template_id: &str,
        requirements: Vec<FieldRequirement>,
        triggers: BTreeMap<String, FieldTrigger>,
    ) {
        self.active_template_id = Some(active_template_id.to_string());
        self.requirements = requirements.into_iter().filter(|r| !r.local).collect();
        self.triggers = triggers;
        self.data.push_dirty = self.sync_enabled;
        self.recompute_fingerprints();
    }

    /// Generate a fresh, unreusable context on commit/activate/cold-start/recovery.
    pub fn new_context(&mut self, reason: &str, now: u64) -> String {
        let counter = self
            .context
            .as_ref()
            .map(|c| c.next_seq.wrapping_add(1))
            .unwrap_or(1);
        let context_id = new_context_id(&self.mac, now, counter);
        self.context = Some(ContextState {
            context_id: context_id.clone(),
            created_at: now,
            reason: reason.into(),
            next_seq: 1,
            acked_push_fp: None,
            acked_full_fp: None,
            full_sync_deadline: now + self.full_sync_s,
            last_applied_seq: 0,
            last_content_crc: None,
        });
        self.data.in_flight = None;
        self.data.push_dirty = self.sync_enabled;
        self.recompute_fingerprints();
        self.last_sequence_reason = Some(reason.into());
        context_id
    }

    pub fn note_snapshot(&mut self, snapshot: SourceSnapshot) {
        self.sources.insert(snapshot.source_id.clone(), snapshot);
        self.recompute_fingerprints();
    }

    fn trigger_of(&self, field: &str) -> FieldTrigger {
        self.triggers
            .get(field)
            .copied()
            .unwrap_or_else(|| crate::datasource::classify_trigger(field))
    }

    /// Recompute bounded fingerprints and decide whether a push is pending.
    pub fn recompute_fingerprints(&mut self) {
        let Some(context) = self.context.clone() else {
            return;
        };
        let merged = merge_sources(&self.sources);
        let closure = |f: &str| self.trigger_of(f);
        let push_fp = fingerprint(&merged.fields, &self.requirements, true, &closure);
        let full_fp = fingerprint(&merged.fields, &self.requirements, false, &closure);
        let push_changed = self.data.push_fp != Some(push_fp);
        self.data.push_fp = Some(push_fp);
        self.data.full_fp = Some(full_fp);
        // Only push-field visible changes trigger delivery; pull-only changes
        // merely update the cache (and piggyback later).
        if push_changed && self.data.acked_push_fp != Some(push_fp) && self.sync_enabled {
            self.data.push_dirty = true;
        }
        let _ = context;
    }

    /// Recompute the complete snapshot from the merged sources.
    pub fn snapshot(&self) -> Option<DataSnapshot> {
        let context = self.context.as_ref()?;
        let merged = merge_sources(&self.sources);
        let fields = assemble_fields(&merged, &self.requirements);
        Some(DataSnapshot {
            active_context_id: context.context_id.clone(),
            data_seq: context.next_seq,
            fields,
        })
    }

    /// True when only pull fields changed since the last confirmed snapshot.
    pub fn pull_only_change(&self) -> bool {
        self.data.acked_full_fp.is_some()
            && self.data.acked_full_fp != self.data.full_fp
            && self.data.acked_push_fp == self.data.push_fp
    }

    pub fn full_sync_due(&self, now: u64) -> bool {
        self.context
            .as_ref()
            .map(|c| now >= c.full_sync_deadline)
            .unwrap_or(false)
    }

    /// Decide the next delivery. `reachable` means an authenticated rendezvous
    /// opportunity exists right now.
    pub fn next_delivery(&mut self, now: u64, reachable: bool) -> Delivery {
        // Mutating operations first, serial per device. A Bundle install is what
        // creates the first context, so it must be deliverable without one.
        let active_job = self
            .job
            .as_ref()
            .filter(|j| matches!(j.state, PublishState::Waiting | PublishState::Sending))
            .map(|j| (j.job_id.clone(), j.state));
        if let Some((job_id, state)) = active_job {
            if !reachable {
                return Delivery::WaitingForRendezvous {
                    reason: format!("bundle job {job_id}"),
                };
            }
            if state == PublishState::Waiting {
                if let Some(j) = self.job.as_mut() {
                    j.state = PublishState::Sending;
                    j.updated_at = now;
                }
            }
            return Delivery::Bundle { job_id };
        }
        if self.context.is_none() {
            return Delivery::Idle;
        }
        if let Some(template_id) = self.pending_activate.clone() {
            if !reachable {
                return Delivery::WaitingForRendezvous {
                    reason: format!("activate {template_id}"),
                };
            }
            let context_id = self
                .context
                .as_ref()
                .map(|c| c.context_id.clone())
                .unwrap_or_default();
            return Delivery::Activate {
                template_id,
                context_id,
            };
        }

        // Data: a pinned in-flight snapshot retries idempotently.
        if let Some(flight) = self.data.in_flight.clone() {
            if !reachable {
                return Delivery::WaitingForRendezvous {
                    reason: format!("data_seq {} unconfirmed", flight.seq),
                };
            }
            return match flight.kind {
                DeliveryKind::BleData => Delivery::BleData(flight.payload),
                _ => Delivery::LightData(flight.payload),
            };
        }

        if !reachable {
            if self.sync_enabled && (self.data.push_dirty || self.full_sync_due(now)) {
                return Delivery::WaitingForRendezvous {
                    reason: if self.data.push_dirty {
                        "push pending".into()
                    } else {
                        "full sync due".into()
                    },
                };
            }
            return Delivery::Idle;
        }

        let want_data = self.sync_enabled && (self.data.push_dirty || self.full_sync_due(now));
        if !want_data {
            return Delivery::Idle;
        }
        let Some(snapshot) = self.snapshot() else {
            return Delivery::Idle;
        };
        let bytes = serde_json::to_vec(&snapshot.fields)
            .map(|v| v.len())
            .unwrap_or(usize::MAX);
        if bytes <= BLE_DATA_BUDGET_BYTES {
            Delivery::BleData(snapshot)
        } else {
            Delivery::LightData(snapshot)
        }
    }

    /// Pin the delivery content (seq assigned here, reused on retries).
    pub fn note_delivery_started(
        &mut self,
        delivery: &Delivery,
        now: u64,
    ) -> Result<InFlight, String> {
        let Some(kind) = delivery.kind() else {
            return Err("delivery has no pinnable payload".into());
        };
        if let Some(flight) = self.data.in_flight.as_mut() {
            // Retry the frozen bytes AND their original fingerprints. A source
            // update during the flight must still be dirty after its ACK.
            flight.attempts = flight.attempts.saturating_add(1);
            flight.kind = kind;
            flight.sent_at = now;
            return Ok(flight.clone());
        }
        let Some(context) = self.context.as_mut() else {
            return Err("no active context".into());
        };
        let payload = match delivery {
            Delivery::BleData(s) | Delivery::LightData(s) => s.clone(),
            _ => DataSnapshot {
                active_context_id: context.context_id.clone(),
                data_seq: context.next_seq,
                fields: BTreeMap::new(),
            },
        };
        let content_crc = format!("{:08x}", data_fields_crc(&wire_fields(&self.requirements, &payload)));
        let flight = InFlight {
            kind,
            seq: context.next_seq,
            content_crc,
            payload,
            push_fp: self.data.push_fp.unwrap_or(0),
            full_fp: self.data.full_fp.unwrap_or(0),
            sent_at: now,
            attempts: 1,
        };
        self.data.in_flight = Some(flight.clone());
        Ok(flight)
    }

    /// Device ACK. Only the pinned content CRC may confirm fingerprints; the
    /// seq only advances after a successful apply, and a stale ACK is ignored.
    pub fn note_ack(
        &mut self,
        kind: DeliveryKind,
        seq: u64,
        content_crc: &str,
        applied: bool,
        _display_state: &str,
        now: u64,
    ) -> AckOutcome {
        let Some(flight) = self.data.in_flight.clone() else {
            return AckOutcome::IgnoredNoFlight;
        };
        if flight.seq != seq || flight.content_crc != content_crc {
            // Old ACK: must not confirm data produced during the transfer.
            return AckOutcome::Stale;
        }
        if !applied {
            return AckOutcome::Rejected;
        }
        match kind {
            DeliveryKind::BleData | DeliveryKind::LightData => {
                let Some(context) = self.context.as_mut() else {
                    return AckOutcome::Stale;
                };
                if flight.payload.active_context_id != context.context_id {
                    return AckOutcome::Stale;
                }
                context.next_seq = seq + 1;
                context.last_applied_seq = seq;
                context.last_content_crc = Some(content_crc.to_string());
                // The ACK confirms exactly the pinned snapshot, never newer data.
                context.acked_push_fp = Some(flight.push_fp);
                context.acked_full_fp = Some(flight.full_fp);
                // Deadline moves only on a confirmed complete snapshot.
                context.full_sync_deadline = now + self.full_sync_s;
                self.data.in_flight = None;
                self.data.acked_push_fp = context.acked_push_fp;
                self.data.acked_full_fp = context.acked_full_fp;
                self.data.push_dirty = self.data.push_fp != Some(flight.push_fp);
                AckOutcome::Applied
            }
            DeliveryKind::Bundle => {
                self.data.in_flight = None;
                if let Some(job) = self.job.as_mut() {
                    job.state = PublishState::Succeeded;
                    job.updated_at = now;
                }
                AckOutcome::Applied
            }
            DeliveryKind::Activate => {
                self.data.in_flight = None;
                self.pending_activate = None;
                AckOutcome::Applied
            }
        }
    }

    // ---- Publishing ------------------------------------------------------

    /// Queue one explicit publish. A frozen bundle replaces an unstarted job;
    /// a job already sending must finish (or be cancelled) first.
    pub fn enqueue_bundle(&mut self, bundle: Bundle) -> Result<(), String> {
        bundle.verify().map_err(|e| e.to_string())?;
        if bundle.device_mac != self.mac {
            return Err(format!(
                "bundle targets {} but coordinator is {}",
                bundle.device_mac, self.mac
            ));
        }
        if !self.caps.supports_render_target(&bundle.render_target) {
            return Err(format!(
                "bundle render_target {} does not match device {}",
                bundle.render_target, self.caps.render_target
            ));
        }
        if bundle.firmware_target != self.caps.firmware_target {
            return Err(format!(
                "bundle firmware_target {} does not match device {}",
                bundle.firmware_target, self.caps.firmware_target
            ));
        }
        if let Some(job) = &self.job {
            match job.state {
                PublishState::Sending => {
                    return Err("a publish is already in progress for this device".into())
                }
                PublishState::Waiting => { /* explicit replace of an unstarted job */ }
                _ => {}
            }
        }
        let now = crate::now_secs();
        self.job = Some(PublishJob {
            job_id: bundle.job_id.clone(),
            device_mac: self.mac.clone(),
            frozen_bundle: bundle,
            state: PublishState::Waiting,
            last_error: None,
            created_at: now,
            updated_at: now,
        });
        Ok(())
    }

    pub fn job_snapshot(&self) -> Option<Value> {
        self.job.as_ref().map(|j| {
            serde_json::json!({
                "job_id": j.job_id,
                "state": j.state,
                "last_error": j.last_error,
                "render_target": j.frozen_bundle.render_target,
                "firmware_target": j.frozen_bundle.firmware_target,
                "template_ids": j.frozen_bundle.profile.template_ids,
                "initial_active": j.frozen_bundle.profile.initial_active_id,
                "crc": j.frozen_bundle.crc,
                "created_at": j.created_at,
            })
        })
    }

    pub fn cancel_job(&mut self) {
        if let Some(job) = self.job.as_mut() {
            if !job.state.is_terminal() {
                job.state = PublishState::Cancelled;
                job.updated_at = crate::now_secs();
            }
        }
        self.data.in_flight = None;
    }

    /// Idempotent result lookup for a retried publish (same frozen content).
    pub fn retry_bundle_ack(&mut self, job_id: &str, committed: bool, now: u64) -> bool {
        let Some(job) = self.job.as_mut() else {
            return false;
        };
        if job.job_id != job_id {
            return false;
        }
        if committed {
            job.state = PublishState::Succeeded;
            job.updated_at = now;
        }
        true
    }

    pub fn request_activate(&mut self, template_id: String) {
        self.pending_activate = Some(template_id);
    }

    // ---- Plans -----------------------------------------------------------

    /// Produce the formal plan for a rendezvous. Duplicate id+content is
    /// idempotent; a *new* id is only generated when the deadline/mode actually
    /// changes. A repeat never restarts the 300 s BOOT window (v2 §7/§12).
    pub fn plan_for(
        &mut self,
        now: u64,
        want_light: bool,
        light_s: u32,
        reason: &str,
    ) -> PowerPlan {
        let light_s = light_s.clamp(30, MAX_LIGHT_S.min(self.caps.max_light_s));
        let desired = if want_light {
            PowerPlan::light(0, light_s, RENDEZVOUS_S, reason)
        } else {
            PowerPlan::sleep(0, RENDEZVOUS_S, reason)
        };
        if let Some(last) = &self.plan.last_sent {
            let same_content = last.mode == desired.mode
                && last.light_duration_s == desired.light_duration_s
                && last.rendezvous_period_s == desired.rendezvous_period_s;
            // Reuse the id only while the accepted window is still comfortably
            // live: after expiry a repeated id would be idempotent and could no
            // longer grant a fresh light window. Before the device has answered,
            // a recently sent plan is also reused so retries stay idempotent.
            let accepted_live = self.plan.last_accepted_id == last.plan_id
                && self
                    .plan
                    .last_accepted_remaining_s
                    .saturating_sub(now.saturating_sub(self.plan.last_accepted_at) as u32)
                    > 60;
            let sent_recently = now.saturating_sub(self.plan.last_sent_at) < 60;
            if same_content && (accepted_live || sent_recently) {
                return last.clone();
            }
        }
        self.plan.next_plan_id += 1;
        let mut plan = desired;
        plan.plan_id = self.plan.next_plan_id;
        self.plan.last_sent = Some(plan.clone());
        self.plan.last_sent_at = now;
        plan
    }

    /// BOOT provisional: the formal plan may shorten/keep/extend; keeping the
    /// original window sends the *remaining* seconds, never a fresh 300.
    pub fn boot_plan(
        &mut self,
        now: u64,
        t_boot: u64,
        want_light: bool,
        light_s: u32,
    ) -> PowerPlan {
        let elapsed = now.saturating_sub(t_boot) as u32;
        let remaining = BOOT_PROVISIONAL_S.saturating_sub(elapsed).max(1);
        if !want_light {
            return self.plan_for(now, false, 0, "boot");
        }
        let max = MAX_LIGHT_S.min(self.caps.max_light_s);
        let duration = if light_s == BOOT_PROVISIONAL_S {
            // Keep the original fallback window: send what is left of it.
            remaining.min(max)
        } else {
            light_s.clamp(30, max)
        };
        if duration < 30 {
            // Too little time left: end the session instead of extending it.
            return self.plan_for(now, false, 0, "boot");
        }
        self.plan_for(now, true, duration, "boot")
    }

    pub fn note_plan_ack(
        &mut self,
        plan_id: u64,
        accepted_remaining_s: u32,
        provisional: bool,
    ) -> PlanAck {
        if plan_id < self.device_plan_high {
            return PlanAck::Stale;
        }
        if plan_id == self.plan.last_accepted_id
            && self.plan.last_accepted_id != 0
            && self.plan.last_sent.as_ref().map(|p| p.plan_id) != Some(plan_id)
        {
            // Same id replayed with no matching sent plan: conflict.
            return PlanAck::Conflict;
        }
        self.device_plan_high = plan_id;
        self.plan.last_accepted_id = plan_id;
        self.plan.last_accepted_remaining_s = accepted_remaining_s;
        self.plan.last_accepted_at = crate::now_secs();
        if self.plan.pending_explicit_light.as_ref().is_some_and(|p| p.plan_id == plan_id) {
            self.plan.pending_explicit_light = None;
            self.plan.last_explicit_light_ack = Some(ExplicitLightAck {
                plan_id, accepted_remaining_s, accepted_at: self.plan.last_accepted_at,
            });
            self.hold_light(self.plan.last_accepted_at + accepted_remaining_s as u64);
        }
        if provisional {
            self.session.power.provisional = true;
        }
        PlanAck::Accepted
    }

    /// Incoming Status digest: the device is authoritative for its context.
    pub fn note_status(&mut self, status: &Value, now: u64) {
        let text = |k: &str| status.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let num = |k: &str| status.get(k).and_then(|v| v.as_u64());
        // An unconfigured device reports an empty id: that is "no context", not
        // a context whose id is the empty string.
        self.session.active_context_id = text("active_context_id").filter(|s| !s.is_empty());
        self.session.active_template_id = text("active_template_id").filter(|s| !s.is_empty());
        self.session.committed_job_id = text("committed_job_id");
        self.session.data_seq = num("data_seq");
        self.session.applied_seq = num("applied_seq");
        self.session.display_state = text("display_state");
        self.session.last_status_at = Some(now);
        if let Some(power) = status.get("power") {
            if let Some(plan_id) = power.get("plan_id").and_then(|v| v.as_u64()) {
                self.device_plan_high = self.device_plan_high.max(plan_id);
            }
            self.session.power.plan_id = self.device_plan_high;
            self.session.power.remaining_s = power
                .get("remaining_s")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            self.session.power.provisional = power
                .get("provisional")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            self.session.power.provisional_remaining_s = power
                .get("provisional_remaining_s")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            self.session.power.mode = match power.get("mode").and_then(|v| v.as_str()) {
                Some("light") => Some(PlanMode::Light),
                Some("sleep") => Some(PlanMode::Sleep),
                _ => self.session.power.mode,
            };
        }
        if let Some(pct) = status
            .get("battery")
            .and_then(|v| v.as_i64())
            .or_else(|| status.get("battery_percent").and_then(|v| v.as_i64()))
        {
            self.session.battery_percent = pct as i32;
            self.session.power.battery_percent = pct as i32;
        }
    }

    /// Context drift detection: an incoming old-context operation is rejected.
    pub fn accepts_context(&self, context_id: &str) -> bool {
        self.context
            .as_ref()
            .map(|c| c.context_id == context_id)
            .unwrap_or(false)
    }

    /// Canonical Data message body for the in-flight snapshot (the device
    /// validates `i` against its own compiled requirement table).
    pub fn data_message_body(&self) -> Option<Value> {
        let flight = self.data.in_flight.as_ref()?;
        let fields = wire_fields(&self.requirements, &flight.payload);
        Some(serde_json::json!({
            "op": "data",
            "active_context_id": flight.payload.active_context_id,
            "seq": flight.seq,
            "crc": format!("{:08x}", data_fields_crc(&fields)),
            "fields": fields,
        }))
    }

    pub fn summary(&self) -> Value {
        let merged = merge_sources(&self.sources);
        serde_json::json!({
            "device_mac": self.mac,
            "caps": self.caps,
            "active_template_id": self.active_template_id,
            "context": self.context.as_ref().map(|c| serde_json::json!({
                "context_id": c.context_id,
                "reason": c.reason,
                "next_seq": c.next_seq,
                "last_applied_seq": c.last_applied_seq,
                "full_sync_deadline": c.full_sync_deadline,
                "acked_push": c.acked_push_fp.is_some(),
                "acked_full": c.acked_full_fp.is_some(),
            })),
            "push_fp": self.data.push_fp,
            "full_fp": self.data.full_fp,
            "acked_push_fp": self.data.acked_push_fp,
            "acked_full_fp": self.data.acked_full_fp,
            "push_dirty": self.data.push_dirty,
            "in_flight": self.data.in_flight.as_ref().map(|f| serde_json::json!({
                "kind": f.kind, "seq": f.seq, "crc": f.content_crc, "attempts": f.attempts,
            })),
            "pull_only_change": self.pull_only_change(),
            "full_sync_due": self.full_sync_due(crate::now_secs()),
            "plan": self.plan,
            "job": self.job_snapshot(),
            "pending_activate": self.pending_activate,
            "session": self.session,
            "source_quality": merged.quality,
            "source_error": merged.error,
            "source_observed_at": merged.observed_at,
            "last_status_at": self.session.last_status_at,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckOutcome {
    Applied,
    Rejected,
    Stale,
    IgnoredNoFlight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanAck {
    Accepted,
    Stale,
    Conflict,
}

/// Non-reusable context id: MAC + counter + time, hashed with a process nonce.
pub fn new_context_id(mac: &str, now: u64, counter: u64) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NONCE: AtomicU64 = AtomicU64::new(0);
    let nonce = NONCE.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in mac
        .bytes()
        .chain(now.to_le_bytes())
        .chain(counter.to_le_bytes())
        .chain(nonce.to_le_bytes())
        .chain(std::process::id().to_le_bytes())
    {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("{:016x}", h)
}

fn crc32(bytes: &[u8]) -> u32 {
    crc32fast::hash(bytes)
}

/// Byte-identical to the device's `v2DataFieldsCrc` (src/v2_runtime.cpp):
/// for each entry in index order `"<i>:<k>:<json value or ~>:<quality>;"`.
pub fn data_fields_crc(fields: &[Value]) -> u32 {
    let mut text = String::new();
    for entry in fields {
        let i = entry.get("i").and_then(|v| v.as_i64()).unwrap_or(-1);
        let k = entry.get("k").and_then(|v| v.as_str()).unwrap_or("");
        let v = entry.get("v").cloned().unwrap_or(Value::Null);
        let q = entry.get("q").and_then(|v| v.as_str()).unwrap_or("missing");
        text.push_str(&i.to_string());
        text.push(':');
        text.push_str(k);
        text.push(':');
        if v.is_null() {
            text.push('~');
        } else {
            text.push_str(&v.to_string());
        }
        text.push(':');
        text.push_str(q);
        text.push(';');
    }
    crc32(text.as_bytes())
}

fn wire_fields(requirements: &[FieldRequirement], payload: &DataSnapshot) -> Vec<Value> {
        let mut entries: Vec<&FieldRequirement> = requirements.iter().collect();
        entries.sort_by_key(|r| r.index);
        let mut fields = Vec::with_capacity(entries.len());
        for r in entries {
            let (value, quality) = match payload.fields.get(&r.field) {
                Some(f) => (f.value.clone(), f.quality),
                None => (Value::Null, Quality::Missing),
            };
            fields.push(serde_json::json!({
                "i": r.index,
                "k": r.field,
                "v": value,
                "q": quality,
            }));
        }
    fields
}

pub fn payload_bytes(payload: &DataSnapshot) -> Vec<u8> {
    crate::template::canonical_bytes(&serde_json::to_value(payload).unwrap_or(Value::Null))
}

/// Merge per-source snapshots: newest value per field wins; the merged quality
/// is the worst of the contributing sources and errors are preserved.
pub fn merge_sources(sources: &BTreeMap<String, SourceSnapshot>) -> SourceSnapshot {
    let mut out = SourceSnapshot {
        source_id: "merged".into(),
        fields: BTreeMap::new(),
        observed_at: 0,
        valid_until: 0,
        last_success_at: 0,
        quality: Quality::Good,
        error: None,
    };
    let mut errors: Vec<String> = Vec::new();
    let mut quality = Quality::Good;
    for snap in sources.values() {
        for (k, v) in &snap.fields {
            let replace = match out.fields.get(k) {
                Some(existing) => v.observed_at >= existing.observed_at,
                None => true,
            };
            if replace {
                out.fields.insert(k.clone(), v.clone());
            }
        }
        if snap.quality != Quality::Good {
            quality = snap.quality;
        }
        if let Some(e) = &snap.error {
            errors.push(format!("{}: {e}", snap.source_id));
        }
        out.observed_at = out.observed_at.max(snap.observed_at);
        out.last_success_at = out.last_success_at.max(snap.last_success_at);
    }
    out.valid_until = sources
        .values()
        .map(|s| s.valid_until)
        .min()
        .unwrap_or(out.observed_at);
    out.quality = quality;
    out.error = if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    };
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datasource::codex_snapshot;
    use crate::platform::model::{FieldKind, MissingPolicy};
    use serde_json::json;

    fn env(used: i64, resets: i64) -> Value {
        json!({
            "schema": 1,
            "server_time": 1_700_000_000,
            "account": {"plan": "plus"},
            "bridge": {"label": "tester", "hostId": "abcd"},
            "buckets": [{"id": "codex", "windows": [
                {"kind": "weekly", "usedPercent": used, "resetsAt": resets, "windowMins": 10080}
            ]}],
            "resetCredits": {"availableCount": 1, "nextExpiresAt": 1_800_000_000}
        })
    }

    fn reqs() -> Vec<FieldRequirement> {
        vec![
            FieldRequirement {
                index: 0,
                field: "buckets[codex].weekly.usedPercent".into(),
                kind: FieldKind::Number,
                missing: MissingPolicy::Hide,
                local: false,
            },
            FieldRequirement {
                index: 1,
                field: "buckets[codex].weekly.resetsAt".into(),
                kind: FieldKind::Number,
                missing: MissingPolicy::Hide,
                local: false,
            },
            FieldRequirement {
                index: 2,
                field: "server_time".into(),
                kind: FieldKind::Number,
                missing: MissingPolicy::Hide,
                local: false,
            },
        ]
    }

    fn coord() -> Coordinator {
        let mut c = Coordinator::new("AA:BB:CC:DD:EE:FF", DeviceCapabilities::ssd1681_154g());
        let triggers = reqs()
            .iter()
            .map(|r| {
                (
                    r.field.clone(),
                    crate::datasource::classify_trigger(&r.field),
                )
            })
            .collect();
        c.sync_enabled = true;
        c.set_contract("quad", reqs(), triggers);
        c.new_context("bundle_commit", 1000);
        c
    }

    #[test]
    fn push_change_queues_full_snapshot_with_pull_fields() {
        let mut c = coord();
        c.note_snapshot(codex_snapshot("codex", &env(30, 1111), 3600));
        let before = c.data.full_fp;
        let d = c.next_delivery(1001, true);
        assert!(matches!(d, Delivery::BleData(_)));
        let Delivery::BleData(snap) = d else {
            unreachable!()
        };
        assert_eq!(
            snap.fields["buckets[codex].weekly.usedPercent"].value,
            json!(30)
        );
        assert_eq!(
            snap.fields["buckets[codex].weekly.resetsAt"].value,
            json!(1111)
        );
        assert_eq!(snap.fields["server_time"].value, json!(1_700_000_000));
        let _ = before;
    }

    #[test]
    fn pull_only_change_does_not_push_or_change_powerplan() {
        let mut c = coord();
        c.note_snapshot(codex_snapshot("codex", &env(30, 1111), 3600));
        let Delivery::BleData(snap) = c.next_delivery(1001, true) else {
            panic!("expected data")
        };
        let flight = c
            .note_delivery_started(&Delivery::BleData(snap.clone()), 1002)
            .unwrap();
        assert_eq!(
            c.note_ack(
                DeliveryKind::BleData,
                flight.seq,
                &flight.content_crc,
                true,
                "displayed",
                1003
            ),
            AckOutcome::Applied
        );
        let deadline = c.context.as_ref().unwrap().full_sync_deadline;
        let plan_id = c.plan.next_plan_id;

        // Reset time (pull) changes: cache only.
        c.note_snapshot(codex_snapshot("codex", &env(30, 2222), 3600));
        assert!(c.pull_only_change());
        assert!(matches!(c.next_delivery(1004, true), Delivery::Idle));
        assert_eq!(c.context.as_ref().unwrap().full_sync_deadline, deadline);
        assert_eq!(c.plan.next_plan_id, plan_id);

        // Push change then piggybacks the latest pull value.
        c.note_snapshot(codex_snapshot("codex", &env(31, 3333), 3600));
        let Delivery::BleData(snap) = c.next_delivery(1005, true) else {
            panic!("expected data")
        };
        assert_eq!(
            snap.fields["buckets[codex].weekly.usedPercent"].value,
            json!(31)
        );
        assert_eq!(
            snap.fields["buckets[codex].weekly.resetsAt"].value,
            json!(3333)
        );
    }

    #[test]
    fn full_sync_deadline_sends_same_value_snapshot() {
        let mut c = coord();
        c.note_snapshot(codex_snapshot("codex", &env(30, 1111), 3600));
        let Delivery::BleData(snap) = c.next_delivery(1001, true) else {
            panic!("expected data")
        };
        let flight = c
            .note_delivery_started(&Delivery::BleData(snap), 1002)
            .unwrap();
        assert_eq!(
            c.note_ack(
                DeliveryKind::BleData,
                flight.seq,
                &flight.content_crc,
                true,
                "displayed",
                1003
            ),
            AckOutcome::Applied
        );
        assert!(matches!(c.next_delivery(1010, true), Delivery::Idle));
        // Deadline reached -> complete sync on the next opportunity.
        assert!(c.full_sync_due(1003 + DEFAULT_FULL_SYNC_S + 1));
        assert!(matches!(
            c.next_delivery(1003 + DEFAULT_FULL_SYNC_S + 1, true),
            Delivery::BleData(_)
        ));
        // Without an opportunity it waits for rendezvous.
        assert!(matches!(
            c.next_delivery(1003 + DEFAULT_FULL_SYNC_S + 2, false),
            Delivery::WaitingForRendezvous { .. }
        ));
    }

    #[test]
    fn ack_loss_retries_same_seq_and_content() {
        let mut c = coord();
        c.note_snapshot(codex_snapshot("codex", &env(30, 1111), 3600));
        let Delivery::BleData(snap) = c.next_delivery(1001, true) else {
            panic!("expected data")
        };
        let flight = c
            .note_delivery_started(&Delivery::BleData(snap), 1002)
            .unwrap();
        // ACK lost. New push arrives while in flight: pinned snapshot retried.
        c.note_snapshot(codex_snapshot("codex", &env(40, 4444), 3600));
        let Delivery::BleData(retry) = c.next_delivery(1003, true) else {
            panic!("expected retry")
        };
        assert_eq!(retry.data_seq, flight.seq);
        assert_eq!(
            retry.fields["buckets[codex].weekly.usedPercent"].value,
            json!(30)
        );
        let retry_flight = c
            .note_delivery_started(&Delivery::BleData(retry), 1004)
            .unwrap();
        assert_eq!(retry_flight.content_crc, flight.content_crc);
        assert_eq!(retry_flight.push_fp, flight.push_fp);
        assert_eq!(retry_flight.full_fp, flight.full_fp);
        assert_eq!(c.note_ack(DeliveryKind::BleData, flight.seq, &flight.content_crc,
                             true, "unchanged", 1005), AckOutcome::Applied);
        assert!(c.data.push_dirty, "ACK for frozen old content must not confirm a newer push");
        let next = c.next_delivery(1006, true);
        let fresh = c.note_delivery_started(&next, 1006).unwrap();
        assert_eq!(fresh.seq, flight.seq + 1);
        assert_ne!(fresh.content_crc, flight.content_crc);
    }

    #[test]
    fn stale_ack_never_confirms_newer_data() {
        let mut c = coord();
        c.note_snapshot(codex_snapshot("codex", &env(30, 1111), 3600));
        let Delivery::BleData(snap) = c.next_delivery(1001, true) else {
            panic!("expected data")
        };
        let flight = c
            .note_delivery_started(&Delivery::BleData(snap), 1002)
            .unwrap();
        let outcome = c.note_ack(
            DeliveryKind::BleData,
            flight.seq,
            "deadbeef",
            true,
            "displayed",
            1003,
        );
        assert_eq!(outcome, AckOutcome::Stale);
        assert!(c.data.in_flight.is_some());
    }

    #[test]
    fn contexts_a_b_a_are_distinct_and_old_rejected() {
        let mut c = coord();
        let a1 = c.context.as_ref().unwrap().context_id.clone();
        let b = c.new_context("activate", 2000);
        assert_ne!(a1, b);
        let a2 = c.new_context("activate", 3000);
        assert_ne!(a1, a2);
        assert_ne!(b, a2);
        assert!(!c.accepts_context(&a1));
        assert!(c.accepts_context(&a2));
    }

    #[test]
    fn plan_id_idempotent_and_never_restarts_boot_window() {
        let mut c = coord();
        let p1 = c.plan_for(1000, true, 300, "rendezvous");
        let p2 = c.plan_for(1005, true, 300, "rendezvous");
        assert_eq!(p1.plan_id, p2.plan_id, "same content reuses the plan id");
        // Repeating an accepted plan does not extend a deadline.
        assert_eq!(c.note_plan_ack(p1.plan_id, 299, false), PlanAck::Accepted);
        let before = c.plan.last_accepted_remaining_s;
        assert_eq!(c.note_plan_ack(p1.plan_id, 299, false), PlanAck::Accepted);
        assert_eq!(c.plan.last_accepted_remaining_s, before);
        // BOOT: keeping the original window sends remaining time, not a fresh 300.
        let mut c2 = coord();
        let boot = c2.boot_plan(1000, 1000, true, 300);
        assert_eq!(boot.light_duration_s, 300);
        let later = c2.boot_plan(1200, 1000, true, 300);
        assert_eq!(later.light_duration_s, 100);
        assert_ne!(later.plan_id, boot.plan_id);
        // A different formal window overrides the remaining time.
        let longer = c2.boot_plan(1200, 1000, true, 600);
        assert_eq!(longer.light_duration_s, 600);
        // Nothing left -> sleep instead of extending the fallback.
        let ended = c2.boot_plan(1400, 1000, true, 300);
        assert_eq!(ended.mode, crate::platform::model::PlanMode::Sleep);
    }

    #[test]
    fn publish_freeze_and_queue_order() {
        use crate::platform::model::{BundleProfile, Template};
        let caps = DeviceCapabilities::ssd1681_154g();
        let compiled = crate::compile::compile(
            &serde_json::from_str(include_str!(
                "../../../../tools/test-bridge/templates/mini.json"
            ))
            .unwrap(),
            &caps.render_target,
        )
        .unwrap();
        let source: Value = serde_json::from_str(include_str!(
            "../../../../tools/test-bridge/templates/mini.json"
        ))
        .unwrap();
        let tpl = Template {
            key: crate::platform::model::TemplateKey::new("mini", &caps.render_target),
            source: source.clone(),
            source_crc: crate::template::template_hash(&crate::template::canonical_bytes(&source)),
            compiled,
            saved_at: 0,
        };
        let bundle = Bundle {
            job_id: "job-1".into(),
            device_mac: "AA:BB:CC:DD:EE:FF".into(),
            bridge_id: String::new(),
            firmware_target: caps.firmware_target.clone(),
            render_target: caps.render_target.clone(),
            compiler_abi: crate::compile::COMPILER_ABI,
            profile: BundleProfile {
                template_ids: vec!["mini".into()],
                initial_active_id: "mini".into(),
                bindings: vec![],
                full_sync_s: 3600,
            },
            templates: vec![tpl],
            resources: vec![],
            total_len: 0,
            crc: String::new(),
        }
        .seal()
        .unwrap();
        let mut c = coord();
        c.enqueue_bundle(bundle.clone()).unwrap();
        assert_eq!(c.job.as_ref().unwrap().state, PublishState::Waiting);
        // Waiting delivery pins the job as sending and delivers the frozen bundle.
        let Delivery::Bundle { job_id } = c.next_delivery(1001, true) else {
            panic!("expected bundle")
        };
        assert_eq!(job_id, "job-1");
        assert_eq!(c.job.as_ref().unwrap().state, PublishState::Sending);
        // A second publish while sending is refused.
        assert!(c.enqueue_bundle(bundle).is_err());
        // Frozen content cannot drift after queueing.
        assert_eq!(
            c.job.as_ref().unwrap().frozen_bundle.templates[0].source,
            source
        );
    }

    #[test]
    fn data_message_fields_are_indexed_and_crc_is_canonical() {
        let mut c = coord();
        c.note_snapshot(codex_snapshot("codex", &env(30, 1111), 3600));
        let delivery = c.next_delivery(1001, true);
        assert!(matches!(delivery, Delivery::BleData(_)));
        c.note_delivery_started(&delivery, 1002).unwrap();
        let body = c.data_message_body().unwrap();
        assert_eq!(
            body["active_context_id"],
            c.context.as_ref().unwrap().context_id
        );
        assert_eq!(body["seq"], 1);
        // Every entry matches the compiled requirement index and field path.
        for entry in body["fields"].as_array().unwrap() {
            let i = entry["i"].as_u64().unwrap() as usize;
            assert_eq!(c.requirements[i].field, entry["k"]);
            assert!(entry["q"].is_string());
        }
        // Cross-implementation fixture: `0:a:1:good;` must hash identically on
        // the device (crates/render/tests/v2_state.rs pins the same vector).
        let fixture = vec![json!({"i": 0, "k": "a", "v": 1, "q": "good"})];
        assert_eq!(data_fields_crc(&fixture), crc32(b"0:a:1:good;"));
        let missing = vec![json!({"i": 1, "k": "b", "v": Value::Null, "q": "missing"})];
        assert_eq!(data_fields_crc(&missing), crc32(b"1:b:~:missing;"));
    }

    #[test]
    fn profile_cycle_visits_every_installed_item() {
        let mut p = crate::platform::model::Profile::draft("AA:BB:CC:DD:EE:FF");
        p.template_ids = (0..8).map(|i| format!("t{i}")).collect();
        p.initial_active_id = Some("t0".into());
        let mut order = Vec::new();
        let mut current = "t0".to_string();
        for _ in 0..8 {
            order.push(current.clone());
            current = p.cycle(&current);
        }
        assert_eq!(order.len(), 8);
        assert_eq!(current, "t0", "cycle wraps");
        for i in 0..8 {
            assert!(order.contains(&format!("t{i}")));
        }
    }

    #[test]
    fn static_source_uses_the_same_snapshot_contract() {
        use crate::datasource::DataSource;
        let ds: DataSource = serde_json::from_value(serde_json::json!({
            "source_id": "static1",
            "kind": "static_json",
            "config": {"values": {
                "buckets[codex].weekly.usedPercent": 40,
                "buckets[codex].weekly.resetsAt": 777,
            }}
        }))
        .unwrap();
        let mut c = coord();
        c.note_snapshot(ds.snapshot(None));
        let Delivery::BleData(snap) = c.next_delivery(1001, true) else {
            panic!("expected data")
        };
        assert_eq!(
            snap.fields["buckets[codex].weekly.usedPercent"].value,
            json!(40)
        );
        assert_eq!(
            snap.fields["buckets[codex].weekly.resetsAt"].value,
            json!(777)
        );
        // Same truth table as Codex: pull-only change does not push.
        let ds2: DataSource = serde_json::from_value(serde_json::json!({
            "source_id": "static1",
            "kind": "static_json",
            "config": {"values": {
                "buckets[codex].weekly.usedPercent": 40,
                "buckets[codex].weekly.resetsAt": 888,
            }}
        }))
        .unwrap();
        let flight = c
            .note_delivery_started(&Delivery::BleData(snap), 1002)
            .unwrap();
        assert_eq!(
            c.note_ack(
                DeliveryKind::BleData,
                flight.seq,
                &flight.content_crc,
                true,
                "displayed",
                1003
            ),
            AckOutcome::Applied
        );
        c.note_snapshot(ds2.snapshot(None));
        assert!(matches!(c.next_delivery(1004, true), Delivery::Idle));
    }

    #[test]
    fn data_reads_and_status_polls_never_extend_light() {
        let mut c = coord();
        let plan = c.plan_for(1000, true, 300, "rendezvous");
        assert_eq!(c.note_plan_ack(plan.plan_id, 300, false), PlanAck::Accepted);
        for i in 0..20 {
            c.note_status(
                &json!({"active_context_id": c.context.as_ref().unwrap().context_id, "power": {"mode": "light", "plan_id": plan.plan_id, "remaining_s": 300}}),
                1001 + i,
            );
        }
        // No implicit renewal: the bridge must issue a new plan to extend.
        let again = c.plan_for(1030, true, 300, "rendezvous");
        assert_eq!(again.plan_id, plan.plan_id);
        let extended = c.plan_for(1030, true, 600, "busy");
        assert_ne!(extended.plan_id, plan.plan_id);
    }

    #[test]
    fn bundle_chunks_are_bounded_and_round_trip() {
        let caps = DeviceCapabilities::ssd1681_154g();
        let source: Value = serde_json::from_str(include_str!(
            "../../../../tools/test-bridge/templates/mini.json"
        ))
        .unwrap();
        let compiled = crate::compile::compile(&source, &caps.render_target).unwrap();
        let bundle = Bundle {
            job_id: "job-chunk".into(),
            device_mac: "AA:BB:CC:DD:EE:FF".into(),
            bridge_id: String::new(),
            firmware_target: caps.firmware_target.clone(),
            render_target: caps.render_target.clone(),
            compiler_abi: crate::compile::COMPILER_ABI,
            profile: crate::platform::model::BundleProfile {
                template_ids: vec!["mini".into()],
                initial_active_id: "mini".into(),
                bindings: vec![],
                full_sync_s: 3600,
            },
            templates: vec![crate::platform::model::Template {
                key: crate::platform::model::TemplateKey::new("mini", &caps.render_target),
                source: source.clone(),
                source_crc: crate::template::template_hash(&crate::template::canonical_bytes(
                    &source,
                )),
                compiled,
                saved_at: 0,
            }],
            resources: vec![],
            total_len: 0,
            crc: String::new(),
        }
        .seal()
        .unwrap();
        bundle.verify().unwrap();
        let chunks = bundle.encode_chunks(180).unwrap();
        assert!(chunks.len() > 2, "payload is split into BEGIN/CHUNK/COMMIT");
        for c in &chunks {
            assert!(
                c.len() <= 182,
                "chunk exceeds payload budget + offset prefix"
            );
        }
        // A tampered copy fails verification.
        let mut tampered = bundle.clone();
        tampered.crc = "00000000".into();
        assert!(tampered.verify().is_err());
    }

    #[test]
    fn target_mismatch_is_rejected() {
        let mut c = coord();
        let caps = DeviceCapabilities::ssd1681_154g();
        let source: Value = serde_json::from_str(include_str!(
            "../../../../tools/test-bridge/templates/mini.json"
        ))
        .unwrap();
        let compiled = crate::compile::compile(&source, "other-target").unwrap();
        let bundle = Bundle {
            job_id: "job-x".into(),
            device_mac: c.mac.clone(),
            bridge_id: String::new(),
            firmware_target: caps.firmware_target.clone(),
            render_target: "other-target".into(),
            compiler_abi: crate::compile::COMPILER_ABI,
            profile: crate::platform::model::BundleProfile {
                template_ids: vec!["mini".into()],
                initial_active_id: "mini".into(),
                bindings: vec![],
                full_sync_s: 3600,
            },
            templates: vec![crate::platform::model::Template {
                key: crate::platform::model::TemplateKey::new("mini", "other-target"),
                source: source.clone(),
                source_crc: crate::template::template_hash(&crate::template::canonical_bytes(
                    &source,
                )),
                compiled,
                saved_at: 0,
            }],
            resources: vec![],
            total_len: 0,
            crc: String::new(),
        }
        .seal()
        .unwrap();
        assert!(c.enqueue_bundle(bundle).is_err());
    }
}
