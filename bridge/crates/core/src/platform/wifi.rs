//! Cached Wi-Fi configurations and resumable jobs. Credentials stay in runtime data.
use anyhow::{Context,bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use super::model::DeviceIdentity;

pub const INTENT_TTL_S: u64 = 86400;
// A finite query-only budget, independent of pre-dispatch authorization/input waits.
const MAX_CONFIRMATION_ATTEMPTS: u32 = 3;
fn zero(value: &u32) -> bool { *value == 0 }

/// Carrier I/O only; job identity and durable transitions belong to the service.
pub trait Transport {
    fn exchange(&mut self, op: &str, body: Value)
        -> impl std::future::Future<Output = Result<Value>> + Send;
}

impl<F: FnMut(&str, Value) -> Result<Value>> Transport for F {
    fn exchange(&mut self, op: &str, body: Value)
        -> impl std::future::Future<Output = Result<Value>> + Send {
        std::future::ready(self(op, body))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionIntent {
    #[default]
    Active,
    Cancelled,
    Expired,
}

#[derive(Clone, Copy)]
pub enum ReplySource { OriginalWrite, ResultQuery }

/// The origin of a response is part of its evidence: a failed result read
/// cannot reject the operation it was trying to observe.
pub enum OperationReply<'a> {
    Committed { revision: u64, bond: Option<&'a str> },
    Completed { revision: Option<u64> },
    Rejected { code: &'a str, durable: bool },
    AcceptedPending,
    Unknown { code: Option<&'a str> },
}
impl<'a> OperationReply<'a> {
    pub fn decode(source: ReplySource, reply: &'a Value) -> Result<Self> {
        let error = reply["error"].as_str();
        if matches!(source, ReplySource::ResultQuery) && reply["ok"] != true {
            return Ok(Self::Unknown { code: Some(error.unwrap_or("receipt_unknown")) });
        }
        if reply["ok"] == true {
            match reply["outcome"].as_str() {
                Some("accepted") => return Ok(Self::AcceptedPending),
                Some("committed") => return Ok(Self::Committed {
                    revision: reply["revision"].as_u64().context("receipt_revision_required")?,
                    bond: reply["bond"].as_str(),
                }),
                Some("completed") => return Ok(Self::Completed { revision: reply["revision"].as_u64() }),
                // Initial control replies expose stage; result queries expose outcome.
                None if matches!(source, ReplySource::OriginalWrite) &&
                    matches!(reply["stage"].as_str(), Some("accepted" | "connecting")) =>
                    return Ok(Self::AcceptedPending),
                _ => {}
            }
        }
        if reply["outcome"] == "rejected" {
            return Ok(Self::Rejected { code: error.unwrap_or("wifi_rejected"), durable: true });
        }
        if reply["ok"] == false && error != Some("storage_unknown") {
            return Ok(Self::Rejected { code: error.unwrap_or("wifi_rejected"), durable: false });
        }
        Ok(Self::Unknown { code: error })
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Network {
    pub network_id: String,
    pub host_ssid: String,
    pub ssid: String,
    #[serde(default)]
    pub label: String,
    #[serde(default = "keep")]
    pub credential: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default = "local")]
    pub origin: String,
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub deleted: bool,
}
impl std::fmt::Debug for Network {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
        f.debug_struct("Network").field("network_id",&self.network_id).field("ssid",&self.ssid)
            .field("version",&self.version).field("password_cached",&self.password.is_some()).finish_non_exhaustive()
    }
}
fn keep() -> String { "keep".into() }
fn local() -> String { "local".into() }
fn id(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
impl Network {
    pub fn validate(&self) -> Result<()> {
        if !id(&self.network_id, 32) || self.host_ssid.is_empty() || self.ssid.is_empty()
            || self.host_ssid.len() > 32 || self.ssid.len() > 32 || self.label.len() > 64
            || [&self.host_ssid, &self.ssid, &self.label, &self.origin].iter().any(|s| s.contains('\0'))
            || !matches!(self.credential.as_str(), "keep" | "replace" | "open" | "device") {
            bail!("invalid_wifi_network");
        }
        if self.password.as_ref().is_some_and(|p|p.contains('\0')||!(p.is_empty()||(8..=63).contains(&p.len())||(p.len()==64&&p.bytes().all(|b|b.is_ascii_hexdigit())))) {
            bail!("invalid_wifi_password");
        }
        Ok(())
    }
    pub fn metadata(&self) -> Value {
        json!({"host_ssid":self.host_ssid,"ssid":self.ssid,"label":self.label,"deleted":self.deleted})
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedNetwork {
    pub bond: String,
    pub writable: bool,
    pub network: Network,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema: u32,
    pub device_mac: String,
    pub revision: u64,
    pub snapshot_id: String,
    pub complete: bool,
    pub observed_at: u64,
    pub networks: Vec<ObservedNetwork>,
    #[serde(default)]
    pub active: Value,
    pub capacity: Value,
}
impl Snapshot {
    fn preferred_network(&self, id: &str) -> Result<&ObservedNetwork> {
        let mut own=self.networks.iter().filter(|n|n.network.network_id==id&&n.writable);
        if let Some(row)=own.next() {
            if own.next().is_some() { bail!("ambiguous_network_id"); }
            return Ok(row);
        }
        let mut source=self.networks.iter().filter(|n|n.network.network_id==id);
        let row=source.next().context("invalid_wifi_selection")?;
        if source.next().is_some() { bail!("ambiguous_network_id"); }
        Ok(row)
    }
    pub fn validate(&self, mac: &str) -> Result<()> {
        if self.schema != 2 || !self.complete || self.snapshot_id.is_empty()
            || DeviceIdentity::normalized_mac(&self.device_mac).as_deref() != Some(mac)
            || !id(&self.snapshot_id, 64) || self.networks.len() > 15 { bail!("invalid_wifi_snapshot"); }
        if self.capacity.as_object().is_none_or(|o|o.iter().any(|(k,v)| !matches!(k.as_str(),"total"|"per_bond")||v.as_u64().is_none())) {
            bail!("invalid_wifi_snapshot");
        }
        if !self.active.is_null() && self.active.as_object().is_none_or(|o|o.iter().any(|(k,v)| {
            match k.as_str() {
                "version" => v.as_u64().is_none(),
                "bond" => v.as_str().is_none_or(|s|s.len()>21||s.contains('\0')),
                "network_id" => v.as_str().is_none_or(|s|!s.is_empty()&&!id(s,32)),
                _ => true,
            }
        })) { bail!("invalid_wifi_snapshot"); }
        let mut seen = BTreeSet::new();
        for n in &self.networks {
            n.network.validate()?;
            if n.bond.is_empty() || n.bond.len() > 21 || n.bond.contains('\0') || !seen.insert((&n.bond, &n.network.network_id)) {
                bail!("invalid_wifi_snapshot");
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub host_ssid: String,
    pub network_id: String,
    pub local_revision: u64,
    pub version: u64,
    pub state: String,
    pub error: Option<String>,
    pub reason: u16,
    pub observed_at: u64,
    pub attempts: u32,
    pub next_attempt_at: u64,
    #[serde(default)]
    pub http_ready: bool,
    #[serde(default)]
    pub bridge_error: Option<String>,
    #[serde(default)]
    pub actual_ssid: Option<String>,
    #[serde(default)]
    pub ip: Option<String>,
}
impl Connection {
    pub fn automatic_state(&self) -> &str {
        if self.attempts >= 3 && !self.http_ready &&
            matches!(self.state.as_str(), "waiting_for_window" | "connection_failed") &&
            !matches!(self.error.as_deref(), Some("authentication_failed" | "security_mismatch")) {
            "retry_exhausted"
        } else {
            &self.state
        }
    }
}

/// Only intent and retry policy are durable. Observations belong to this process
/// and are invalidated on reboot; they cannot restore a fictitious connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionFacts {
    pub schema: u8,
    pub host_ssid: String,
    pub network_id: String,
    pub local_revision: u64,
    pub version: u64,
    pub requested: bool,
    pub attempts: u32,
    pub next_attempt_at: u64,
    pub admission_error: Option<String>,
    #[serde(skip)]
    pub observation: Option<NetworkObservation>,
    #[serde(skip)]
    pub contact: Option<HttpContact>,
}
#[derive(Debug, Clone)]
pub struct NetworkObservation {
    pub boot_id: String,
    pub at: u64,
    pub state: String,
    pub error: Option<String>,
    pub reason: u16,
    pub ssid: Option<String>,
    pub ip: Option<String>,
}
#[derive(Debug, Clone)]
pub struct HttpContact { pub boot_id: String, pub at: u64, pub error: Option<String> }
impl ConnectionFacts {
    pub fn new(target: &Network, revision: u64) -> Self {
        Self { schema:2, host_ssid:target.host_ssid.clone(), network_id:target.network_id.clone(),
            local_revision:revision, version:0, requested:false, attempts:0, next_attempt_at:0,
            admission_error:None, observation:None, contact:None }
    }
    pub fn view(&self, now: u64, jobs: &[Job]) -> Connection {
        let observed=self.observation.as_ref().filter(|o|now>=o.at && now-o.at<=360);
        let contact=self.contact.as_ref().filter(|c|now>=c.at && now-c.at<=360 &&
            observed.is_none_or(|o|o.boot_id==c.boot_id && c.at>=o.at));
        let pending=jobs.iter().rev().find(|j|matches!(j.kind.as_str(),"trial"|"activate") &&
            j.command.as_ref().is_some_and(|v|v["network_id"]==self.network_id) && !j.terminal());
        let http_ready=!self.requested && contact.is_some_and(|c|c.error.is_none());
        let state=if self.requested {"requested"} else if http_ready {"connected"}
            else if let Some(job)=pending {if !job.sent {"waiting_for_window"}else if job.permits_network_start(){"connecting"}else{"awaiting_confirmation"}}
            else if let Some(o)=observed {&o.state} else {"waiting_for_window"};
        Connection { host_ssid:self.host_ssid.clone(),network_id:self.network_id.clone(),
            local_revision:self.local_revision,version:self.version,state:state.into(),
            error:if http_ready || self.requested {None}else{self.admission_error.clone().or_else(||observed.and_then(|o|o.error.clone()))},
            reason:if http_ready {0}else{observed.map_or(0,|o|o.reason)}, observed_at:observed.map_or(0,|o|o.at),
            attempts:self.attempts,next_attempt_at:self.next_attempt_at,http_ready,
            bridge_error:contact.and_then(|c|c.error.clone()),actual_ssid:observed.and_then(|o|o.ssid.clone()),
            ip:observed.and_then(|o|o.ip.clone()) }
    }
    pub fn note_http(&mut self, boot_id: String, at: u64, error: Option<String>) {
        if self.observation.as_ref().is_some_and(|o|o.boot_id!=boot_id) {self.observation=None;}
        if error.is_none() && !self.requested {self.attempts=0;self.next_attempt_at=0;self.admission_error=None;}
        self.contact=Some(HttpContact{boot_id,at,error});
    }
    pub fn from_legacy(c: Connection) -> Self {
        Self {schema:2,host_ssid:c.host_ssid,network_id:c.network_id,local_revision:c.local_revision,
            version:c.version,requested:c.state=="requested",attempts:c.attempts,next_attempt_at:c.next_attempt_at,
            admission_error:c.error.filter(|e|matches!(e.as_str(),"authentication_failed"|"security_mismatch")),observation:None,contact:None}
    }
}
fn connection_from_disk<'de,D:serde::Deserializer<'de>>(d:D)->std::result::Result<Option<ConnectionFacts>,D::Error> {
    let value=Option::<Value>::deserialize(d)?;
    value.map(|value| {
        if value.get("schema").is_none() {
            serde_json::from_value::<Connection>(value).map(ConnectionFacts::from_legacy).map_err(serde::de::Error::custom)
        } else if value["schema"]==2 {serde_json::from_value(value).map_err(serde::de::Error::custom)}
        else {Err(serde::de::Error::custom("unsupported_wifi_connection_schema"))}
    }).transpose()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WifiState {
    /// AP01 list-write uncertainty fence. Metadata only, never passwords or menu codes.
    #[serde(default)]
    pub ap01_pending: Option<Value>,
    #[serde(default)]
    pub next_job_seq: u64,
    pub local_revision: u64,
    pub saved_at: u64,
    pub targets: Vec<Network>,
    pub snapshot: Option<Snapshot>,
    pub baseline: BTreeMap<String, Value>,
    pub jobs: Vec<Job>,
    #[serde(default, rename="connection", deserialize_with="connection_from_disk")]
    pub connection_facts: Option<ConnectionFacts>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settlement {
    pub attempts: u32,
    pub next_attempt_at: u64,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if="Option::is_none")]
    pub result_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub job_id: String,
    pub request_id: String,
    pub device_mac: String,
    pub bridge_id: String,
    #[serde(default, skip_serializing_if="Option::is_none")]
    pub transaction_namespace: Option<String>,
    pub kind: String,
    pub state: String,
    #[serde(default)]
    pub execution_intent: ExecutionIntent,
    pub created_at: u64,
    pub expires_at: u64,
    pub updated_at: u64,
    pub local_revision: u64,
    pub attempt_count: u32,
    #[serde(default, skip_serializing_if="zero")]
    pub confirmation_attempts: u32,
    pub last_attempt_at: Option<u64>,
    pub next_attempt_at: u64,
    pub last_stage: String,
    pub error_code: Option<String>,
    pub sent: bool,
    pub may_have_committed: bool,
    pub selected: Vec<String>,
    pub frozen: Vec<Network>,
    #[serde(default)]
    pub fields: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub host_ssid: Option<String>,
    pub before: BTreeMap<String, Value>,
    pub confirmed_revision: Option<u64>,
    #[serde(default)]
    pub confirmation: Option<String>,
    #[serde(default)]
    pub receipt_ack_pending: bool,
    #[serde(default, skip_serializing_if="Option::is_none")]
    pub settlement: Option<Settlement>,
    #[serde(default, skip_serializing_if="Option::is_none")]
    pub command: Option<Value>,
}
impl Job {
    /// Only explicit acceptance or completion permits the caller's network start.
    pub fn permits_network_start(&self) -> bool {
        matches!(self.kind.as_str(), "trial" | "activate") && self.sent && self.error_code.is_none() &&
            (self.state == "completed" || (self.state == "awaiting_confirmation" && self.last_stage == "accepted"))
    }
    /// Historical reconciliation is query-only and never creates a wake intent.
    pub fn settlement_due(&self, now: u64) -> bool {
        self.terminal() && (self.receipt_ack_pending || self.confirmation_pending())
            && self.request_id.starts_with("w3_")
            && self.transaction_namespace.as_deref().is_some_and(|s| !s.is_empty())
            && self.settlement.as_ref().is_none_or(|s| s.attempts < 3 && now >= s.next_attempt_at)
    }
    pub fn check_transaction_context(&self, principal: &str, namespace: &str) -> Result<()> {
        if principal != self.bridge_id { bail!("transaction_principal_changed"); }
        if namespace.is_empty() || namespace.len() > 21 { bail!("transaction_namespace_invalid"); }
        match self.transaction_namespace.as_deref() {
            Some(saved) if saved == namespace => Ok(()),
            Some(_) => bail!("transaction_namespace_changed"),
            None => bail!("transaction_identity_unknown"),
        }
    }
    pub fn confirmation_pending(&self) -> bool {
        self.sent && self.may_have_committed && !matches!(self.confirmation.as_deref(),
            Some("durable_receipt" | "operation_result" | "durable_rejection" | "receipt_and_readback" | "verified_post_state"))
    }
    /// Unresolved evidence belongs to history after execution has ended.
    pub fn blocks_mutations(&self) -> bool {
        !self.terminal() && self.confirmation_pending()
    }
    pub fn execution_active(&self, now: u64) -> bool {
        self.execution_intent == ExecutionIntent::Active && now < self.expires_at
            && !matches!(self.state.as_str(), "cancelled" | "expired")
    }
    fn safe_to_retire(&self) -> bool {
        self.terminal() && !self.receipt_ack_pending && (!self.may_have_committed ||
            matches!(self.confirmation.as_deref(),Some("durable_receipt"|"operation_result"|"durable_rejection"|"receipt_and_readback"|"verified_post_state")))
    }
    pub fn terminal(&self) -> bool {
        matches!(self.state.as_str(), "completed" | "cancelled" | "expired" | "failed")
    }
    pub fn runnable(&self, now: u64) -> bool {
        now >= self.next_attempt_at && self.execution_active(now) && !self.terminal()
            && !matches!(self.state.as_str(), "conflict" | "needs_attention")
            && self.blocking_reason().is_none()
    }
    fn blocking_reason(&self) -> Option<&str> {
        if self.confirmation_pending() && self.transaction_namespace.as_deref().is_none_or(|s|s.is_empty()) {
            return Some("transaction_identity_unknown");
        }
        self.error_code.as_deref().filter(|code|matches!(*code,
            "transaction_identity_unknown" | "transaction_principal_changed" |
            "transaction_namespace_changed" | "transaction_namespace_invalid"))
    }
    fn reconcile_execution(&mut self) {
        // Execution intent is independent of the historical, possibly applied result.
        if matches!(self.state.as_str(), "completed" | "failed") { return; }
        if self.execution_intent == ExecutionIntent::Cancelled { self.state="cancelled".into(); }
        else if self.execution_intent == ExecutionIntent::Expired { self.state="expired".into(); }
        else if !self.terminal() {
            if let Some(reason)=self.blocking_reason().map(str::to_owned) {
                self.state="failed".into();self.error_code=Some(reason);
            } else if self.confirmation_pending() && self.confirmation_attempts >= MAX_CONFIRMATION_ATTEMPTS {
                self.state="failed".into();self.error_code=Some("confirmation_budget_exhausted".into());
            }
        }
    }
}

#[cfg(test)]
mod retirement_tests {
    use super::*;
    #[test]
    fn accepted_evidence_respects_reply_source() {
        for stage in ["accepted", "connecting"] {
            let reply=json!({"ok":true,"stage":stage});
            assert!(matches!(OperationReply::decode(ReplySource::OriginalWrite,&reply).unwrap(),OperationReply::AcceptedPending));
            assert!(matches!(OperationReply::decode(ReplySource::ResultQuery,&reply).unwrap(),OperationReply::Unknown{..}));
        }
        for source in [ReplySource::OriginalWrite,ReplySource::ResultQuery] {
            for reply in [json!({"ok":true,"outcome":"unknown","stage":"accepted"}),
                json!({"ok":true,"outcome":"pending"}),json!({"ok":true})] {
                assert!(matches!(OperationReply::decode(source,&reply).unwrap(),OperationReply::Unknown{..}));
            }
        }
    }
    #[test]
    fn unknown_wifi_results_do_not_revive_cancelled_expired_or_unidentifiable_jobs() {
        for reason in ["cancelled", "expired", "missing", "changed"] {
            let mut state=WifiState::default();
            let job=state.queue("0200000000D4","bridge-test","activate",vec!["home".into()],100,"first".into(),BTreeMap::new()).unwrap();
            state.jobs[0].transaction_namespace=Some("original".into());
            state.mark_sent(&job.job_id,101).unwrap();
            state.finish(&job.job_id,"awaiting_confirmation",Some("result_unknown"),102).unwrap();
            assert!(state.jobs[0].runnable(200));
            match reason {
                "cancelled"=>{state.finish(&job.job_id,"cancelled",None,201).unwrap();},
                "expired"=>{state.jobs[0].expires_at=201;state.expire(202);},
                "missing"=>{state.jobs[0].transaction_namespace=None;},
                _=>{state.finish(&job.job_id,"awaiting_confirmation",Some("transaction_namespace_changed"),201).unwrap();},
            }
            assert!(!state.jobs[0].runnable(300),"{reason} must not automatically retry");
            state.recover();
            let before=state.jobs[0].state.clone();let error=state.jobs[0].error_code.clone();
            state.finish(&job.job_id,"awaiting_confirmation",Some("rendezvous_failed"),400).unwrap();
            assert_eq!(state.jobs[0].state,before,"late transport failure must not revive {reason}");
            assert_eq!(state.jobs[0].error_code,error,"late transport failure must preserve {reason}");
            assert!(state.jobs[0].confirmation_pending());
            assert!(!state.jobs[0].safe_to_retire());
            assert_eq!(state.jobs[0].request_id,job.request_id);
            // A real receipt arriving from an already in-flight call still wins;
            // stopping future attempts must not erase a proven outcome on restart.
            state.jobs[0].state="completed".into();
            state.jobs[0].confirmation=Some("operation_result".into());
            state.recover();
            assert_eq!(state.jobs[0].state,"completed");
        }
    }
    #[test]
    fn unknown_confirmation_has_a_finite_execution_budget() {
        let mut state=WifiState::default();
        let job=state.queue("0200000000D4","bridge-test","activate",vec!["home".into()],100,"first".into(),BTreeMap::new()).unwrap();
        state.jobs[0].transaction_namespace=Some("original".into());
        for attempt in 0..4 {
            let now=101+attempt*61;
            assert!(state.begin(&job.job_id,now).unwrap().is_some());
            if attempt==0 {state.mark_sent(&job.job_id,now).unwrap();}
            state.finish(&job.job_id,"awaiting_confirmation",Some("result_unknown"),now).unwrap();
        }
        let historical=state.jobs[0].clone();
        assert_eq!(historical.state,"failed");
        assert_eq!(historical.error_code.as_deref(),Some("confirmation_budget_exhausted"));
        assert!(historical.confirmation_pending());
        assert!(!historical.runnable(500));
        state.queue("0200000000D4","bridge-test","activate",vec!["office".into()],500,"next".into(),BTreeMap::new()).unwrap();
        assert_eq!(serde_json::to_value(&state.jobs[0]).unwrap(),serde_json::to_value(historical).unwrap());
    }
    #[test]
    fn wifi_retirement_preserves_unknown_submissions_and_pending_receipt_acks() {
        let mut state=WifiState::default();
        let job=state.queue("0200000000D4","bridge-test","activate",vec!["home".into()],100,"first".into(),BTreeMap::new()).unwrap();
        let mut template=job;template.state="completed".into();template.sent=true;template.may_have_committed=true;
        template.confirmation=Some("operation_result".into());template.receipt_ack_pending=true;
        state.jobs=(0..32).map(|i|{let mut j=template.clone();j.job_id=format!("protected-{i}");j.request_id=j.job_id.clone();
            if i%2==0{j.state="expired".into();j.confirmation=None;j.receipt_ack_pending=false;}j}).collect();
        let before=serde_json::to_value(&state.jobs).unwrap();
        assert!(state.queue("0200000000D4","bridge-test","read",vec![],1_000_000,"next".into(),BTreeMap::new()).unwrap_err().to_string().contains("wifi_job_capacity"));
        assert_eq!(serde_json::to_value(&state.jobs).unwrap(),before);
        state.jobs[1].receipt_ack_pending=false;
        state.queue("0200000000D4","bridge-test","read",vec![],1_000_000,"next".into(),BTreeMap::new()).unwrap();
        assert_eq!(state.jobs.len(),32);
        assert!(!state.jobs.iter().any(|j|j.job_id=="protected-1"));
        assert!(state.jobs.iter().any(|j|j.job_id=="protected-0"));
    }
}
impl WifiState {
    pub fn matching_target(&self, host_ssid: &str) -> Option<&Network> {
        let mut rows=self.targets.iter().filter(|n|!n.deleted && n.origin=="local" && n.host_ssid==host_ssid);
        let row=rows.next()?;
        rows.next().is_none().then_some(row)
    }
    pub fn matching_connection(&self, host_ssid: &str) -> Option<&ConnectionFacts> {
        let target=self.matching_target(host_ssid)?;
        self.connection_facts.as_ref().filter(|c| c.host_ssid==host_ssid && c.network_id==target.network_id && c.local_revision==self.local_revision)
    }
    pub fn connection_at(&self, now: u64) -> Option<Connection> {
        self.connection_facts.as_ref().filter(|c|c.local_revision==self.local_revision)
            .map(|c|c.view(now,&self.jobs))
    }
    #[cfg(test)]
    pub fn connection(&self) -> Option<Connection> {
        let at=self.connection_facts.as_ref().map_or(0,|c|c.observation.as_ref().map_or(0,|o|o.at)
            .max(c.contact.as_ref().map_or(0,|h|h.at)));
        self.connection_at(at)
    }
    pub fn recover(&mut self) {
        for j in &mut self.jobs {
            // The old decoder also used operation_pending for missing outcomes;
            // it is not evidence of acceptance. Preserve the original query ID.
            if j.error_code.as_deref() == Some("operation_pending") {
                j.error_code = Some("result_unknown".into());
            }
            if j.state == "cancelled" { j.execution_intent = ExecutionIntent::Cancelled; }
            if j.state == "expired" { j.execution_intent = ExecutionIntent::Expired; }
            j.reconcile_execution();
            if matches!(j.state.as_str(), "connecting" | "reading" | "applying") {
                j.state = if j.sent { "awaiting_confirmation" } else { "waiting_rendezvous" }.into();
            }
            if !j.terminal() && !j.sent && !matches!(j.state.as_str(),"conflict"|"needs_attention") && j.frozen.iter().any(|n|!n.deleted && n.credential=="replace" && n.password.is_none()) {
                j.state="waiting_input".into();j.error_code=Some("credentials_required".into());
            }
        }
    }
    pub fn save(&mut self, expected: u64, mut targets: Vec<Network>, now: u64) -> Result<()> {
        if expected != self.local_revision { bail!("local_revision_conflict"); }
        if targets.len() > 16 { bail!("capacity"); }
        let mut ids = BTreeSet::new();
        let mut hosts = BTreeSet::new();
        for n in &mut targets {
            // Device binding ownership belongs to the snapshot, not imported local configurations.
            n.origin = "local".into();
            n.validate()?;
            if !ids.insert(&n.network_id) || (!n.deleted && n.origin == "local" && !hosts.insert(&n.host_ssid)) {
                bail!("mapping_conflict");
            }
        }
        self.targets = targets;
        self.local_revision = self.local_revision.checked_add(1).ok_or_else(|| anyhow::anyhow!("revision_exhausted"))?;
        self.saved_at = now;
        Ok(())
    }
    pub fn expire(&mut self, now: u64) {
        for j in &mut self.jobs {
            j.reconcile_execution();
            if !j.terminal() && j.execution_intent == ExecutionIntent::Active && now >= j.expires_at {
                j.execution_intent = ExecutionIntent::Expired;
                j.state = "expired".into(); j.updated_at = now;
            }
        }
    }
    pub fn queue(&mut self, mac: &str, bridge: &str, kind: &str, selected: Vec<String>, now: u64, job_id: String, fields: BTreeMap<String, Vec<String>>) -> Result<Job> {
        if !matches!(kind, "read" | "sync" | "read_import" | "recovery_save" | "trial" | "activate") || bridge.is_empty() || bridge.len() > 64 {
            bail!("invalid_wifi_intent");
        }
        self.expire(now);
        if fields.iter().any(|(id, fs)| !selected.contains(id) || fs.is_empty() || fs.iter().collect::<BTreeSet<_>>().len() != fs.len() || fs.iter().any(|f|
            !matches!(f.as_str(),"host_ssid"|"ssid"|"label"|"deleted"|"credential"))) { bail!("invalid_wifi_fields"); }
        let frozen: Vec<_> = if kind == "read" { vec![] } else {
            if selected.is_empty() || selected.len() > 4 { bail!("invalid_wifi_selection"); }
            let unique: BTreeSet<_> = selected.iter().collect();
            if unique.len() != selected.len() { bail!("invalid_wifi_selection"); }
            if kind == "sync" || kind == "recovery_save" {
                selected.iter().map(|id| self.targets.iter().find(|n| &n.network_id == id)
                    .cloned().ok_or_else(|| anyhow::anyhow!("invalid_wifi_selection"))).collect::<Result<_>>()?
            } else { vec![] }
        };
        if kind == "recovery_save" && (frozen.len() != 1 || frozen[0].deleted || frozen[0].origin != "local") { bail!("invalid_recovery_intent"); }
        if kind == "sync" && (self.snapshot.is_none() || frozen.iter().any(|n| n.origin != "local")) {
            bail!("snapshot_required_or_read_only");
        }
        if let Some(j) = self.jobs.iter().find(|j| !j.terminal() && j.kind == kind && j.bridge_id == bridge
            && j.selected == selected && j.fields == fields && j.frozen == frozen && (kind == "read" || j.local_revision == self.local_revision)) {
            return Ok(j.clone());
        }
        if matches!(kind,"sync"|"recovery_save"|"trial"|"activate") &&
            self.jobs.iter().any(Job::blocks_mutations) { bail!("wifi_confirmation_pending"); }
        if matches!(kind,"sync"|"recovery_save") && self.jobs.iter().any(|j| matches!(j.kind.as_str(),"sync"|"recovery_save") && !j.terminal()) { bail!("wifi_sync_busy"); }
        self.jobs.retain(|j| !j.safe_to_retire() || now.saturating_sub(j.updated_at) < INTENT_TTL_S);
        // Keep recent history when possible; confirmed, acknowledged results
        // must not prevent new work. Unknown submissions and pending ACKs stay.
        while self.jobs.len() >= 32 {
            let Some(index)=self.jobs.iter().enumerate().filter(|(_,j)|j.safe_to_retire())
                .min_by_key(|(_,j)|j.updated_at).map(|(i,_)|i) else{break;};
            self.jobs.remove(index);
        }
        if self.jobs.len() >= 32 { bail!("wifi_job_capacity"); }
        let before = self.snapshot.as_ref().map(|s| s.networks.iter().filter(|n| n.writable && selected.contains(&n.network.network_id))
            .map(|n| (n.network.network_id.clone(), n.network.metadata())).collect()).unwrap_or_default();
        let j = Job { request_id: if self.next_job_seq>0 { format!("w3_{:016x}",self.next_job_seq) } else { job_id.clone() }, job_id, device_mac: mac.into(), bridge_id: bridge.into(), kind: kind.into(),
            transaction_namespace: None, state: "queued".into(), execution_intent: ExecutionIntent::Active, created_at: now, expires_at: now.saturating_add(INTENT_TTL_S), updated_at: now,
            local_revision: self.local_revision, attempt_count: 0, confirmation_attempts: 0, last_attempt_at: None, next_attempt_at: now,
            last_stage: "accepted".into(), error_code: None, sent: false, may_have_committed: false,
            selected, frozen, fields, expected_revision: self.snapshot.as_ref().map(|s| s.revision), host_ssid: None, before, confirmed_revision: None, confirmation: None, receipt_ack_pending: false, settlement: None, command: None };
        self.jobs.push(j.clone());
        Ok(j)
    }
    pub fn begin(&mut self, job: &str, now: u64) -> Result<Option<Job>> {
        self.expire(now);
        let j = self.jobs.iter_mut().find(|j| j.job_id == job).ok_or_else(|| anyhow::anyhow!("wifi_job_not_found"))?;
        if !j.runnable(now) { return Ok(None); }
        if matches!(j.state.as_str(), "connecting" | "reading" | "applying") { return Ok(None); }
        j.state = if j.sent { "awaiting_confirmation" } else { "reading" }.into();
        j.last_stage = if j.sent { "query_result" } else { "snapshot" }.into();
        if j.sent { j.confirmation_attempts = j.confirmation_attempts.saturating_add(1); }
        j.attempt_count = j.attempt_count.saturating_add(1); j.last_attempt_at = Some(now); j.updated_at = now;
        Ok(Some(j.clone()))
    }
    pub fn finish(&mut self, job: &str, state: &str, error: Option<&str>, now: u64) -> Result<Job> {
        if !matches!(state, "waiting_rendezvous" | "waiting_authorization" | "waiting_input" | "awaiting_confirmation" | "conflict" | "failed" | "cancelled") {
            bail!("invalid_wifi_job_state");
        }
        if error.is_some_and(|s| !id(s, 64)) { bail!("invalid_error_code"); }
        let j = self.jobs.iter_mut().find(|j| j.job_id == job).ok_or_else(|| anyhow::anyhow!("wifi_job_not_found"))?;
        j.reconcile_execution();
        if state != "cancelled" && (j.terminal() || j.state == "needs_attention") { return Ok(j.clone()); }
        if !j.terminal() {
            if state == "cancelled" { j.execution_intent = ExecutionIntent::Cancelled; }
            j.state = state.into();
            if state != "cancelled" { j.error_code = error.map(str::to_owned); }
            j.updated_at = now;
            j.reconcile_execution();
            let delay = match j.attempt_count { 0 | 1 => 5, 2 => 15, _ => 60 };
            j.next_attempt_at = now.saturating_add(delay);
        }
        Ok(j.clone())
    }
    pub fn complete_read(&mut self, job: &str, snapshot: Snapshot, now: u64) -> Result<Job> {
        let j = self.jobs.iter().find(|j| j.job_id == job).ok_or_else(|| anyhow::anyhow!("wifi_job_not_found"))?.clone();
        snapshot.validate(&j.device_mac)?;
        if j.terminal() || now >= j.expires_at { bail!("wifi_job_no_longer_active"); }
        // Always keep the last full observation. An import rechecks local revision.
        self.snapshot = Some(snapshot.clone());
        if j.kind == "read_import" {
            if j.local_revision != self.local_revision { return self.finish(job, "conflict", Some("local_revision_conflict"), now); }
            self.import_fields(&j.selected, &j.fields, self.local_revision, now)?;
        }
        let j = self.jobs.iter_mut().find(|j| j.job_id == job).unwrap();
        j.state = "completed".into(); j.last_stage = "complete_snapshot".into(); j.updated_at = now;
        j.error_code = None; j.confirmed_revision = Some(snapshot.revision);
        Ok(j.clone())
    }
    pub fn mark_sent(&mut self, job: &str, now: u64) -> Result<()> {
        let j = self.jobs.iter_mut().find(|j| j.job_id == job).ok_or_else(|| anyhow::anyhow!("wifi_job_not_found"))?;
        if j.terminal() || now >= j.expires_at { bail!("wifi_job_no_longer_active"); }
        j.sent = true; j.may_have_committed = true; j.state = "applying".into();
        j.last_stage = "request_sent".into(); j.updated_at = now; Ok(())
    }
    pub fn confirm_sync(&mut self, job: &str, snapshot: Snapshot, receipt: bool, now: u64) -> Result<Job> {
        let j = self.jobs.iter().find(|j| j.job_id == job).ok_or_else(|| anyhow::anyhow!("wifi_job_not_found"))?.clone();
        snapshot.validate(&j.device_mac)?;
        if !j.confirmation_pending() { bail!("wifi_job_no_longer_active"); }
        for n in &j.frozen {
            let matches: Vec<_> = snapshot.networks.iter().filter(|r| r.writable && r.network.network_id == n.network_id).collect();
            let deleted = n.deleted && j.fields.get(&n.network_id).is_none_or(|fs| fs.iter().any(|f| f == "deleted"));
            if (deleted && !matches.is_empty()) || (!deleted && (matches.len() != 1 || j.fields.get(&n.network_id).map_or(matches[0].network.metadata() != n.metadata(), |fields|
                    fields.iter().any(|f| f != "credential" && matches[0].network.metadata()[f] != n.metadata()[f])))) {
                return self.finish(job, "awaiting_confirmation", Some("target_unconfirmed"), now);
            }
        }
        if !receipt && j.frozen.iter().any(|n| !n.deleted && matches!(n.credential.as_str(), "replace" | "open") &&
            j.fields.get(&n.network_id).is_none_or(|fs| fs.iter().any(|f| f == "credential"))) {
            self.snapshot = Some(snapshot);
            return self.finish(job, "awaiting_confirmation", Some("credential_unconfirmed"), now);
        }
        for n in &j.frozen {
            let metadata = n.metadata();
            if let Some(fields) = j.fields.get(&n.network_id) {
                let base = self.baseline.entry(n.network_id.clone()).or_insert(json!({}));
                for f in fields { if f != "credential" { base[f] = metadata[f].clone(); } }
            } else { self.baseline.insert(n.network_id.clone(), metadata); }
        }
        // A fulfilled whole-row intent must not cause another password write on
        // every rendezvous. Never consume newer local edits or partial jobs.
        if self.local_revision==j.local_revision {
            for frozen in &j.frozen {
                if j.fields.contains_key(&frozen.network_id)||frozen.deleted {continue;}
                if let Some(target)=self.targets.iter_mut().find(|n|n.network_id==frozen.network_id&&n.metadata()==frozen.metadata()&&n.credential==frozen.credential) {
                    if let Some(row)=snapshot.networks.iter().find(|n|n.writable&&n.network.network_id==frozen.network_id) {
                        target.version=row.network.version;
                        if matches!(target.credential.as_str(),"replace"|"open")&&receipt {target.credential="keep".into();}
                    }
                }
            }
        }
        self.snapshot = Some(snapshot.clone());
        let j = self.jobs.iter_mut().find(|j| j.job_id == job).unwrap();
        j.state = "completed".into(); j.updated_at = now; j.last_stage = "readback_verified".into();
        j.confirmed_revision = Some(snapshot.revision); j.error_code = None;
        j.confirmation = Some(if receipt { "receipt_and_readback" } else { "target_verified_transaction_unknown" }.into());
        Ok(j.clone())
    }
    /// Manual page guard only: merge the selected whole rows into the cached
    /// device state. Never auto-select a row or change the active network.
    pub fn validate_manual_sync(&self, selected: &[String], host: &str, snapshot_id: &str) -> Result<()> {
        let snapshot = self.snapshot.as_ref().context("snapshot_required")?;
        if snapshot.snapshot_id != snapshot_id { bail!("snapshot_conflict"); }
        if host.is_empty() { bail!("host_network_unknown"); }
        let retained = snapshot.networks.iter().any(|r| r.network.host_ssid == host &&
            !r.network.deleted && (!r.writable || !selected.contains(&r.network.network_id)));
        let included = self.targets.iter().any(|n| selected.contains(&n.network_id) &&
            n.origin == "local" && !n.deleted && n.host_ssid == host && !n.ssid.is_empty());
        if !retained && !included { bail!("current_network_required"); }
        Ok(())
    }
    /// A durable receipt is written in the same verified NVS commit as the
    /// frozen changes. Apply those changes to the cache, without another read.
    pub fn confirm_receipt(&mut self, job_id: &str, revision: u64, bond: Option<&str>, now: u64) -> Result<Job> {
        let job = self.jobs.iter().find(|j| j.job_id == job_id).context("wifi_job_not_found")?.clone();
        if !job.confirmation_pending() { bail!("wifi_job_no_longer_active"); }
        if job.expected_revision.is_some_and(|r| revision != r.saturating_add(1)) { bail!("receipt_revision_mismatch"); }
        if job.kind == "sync" {
            let mut snapshot = self.snapshot.clone().context("snapshot_required")?;
            if job.expected_revision != Some(snapshot.revision) { bail!("snapshot_conflict"); }
            let own_bond = bond.map(str::to_owned).or_else(|| snapshot.networks.iter().find(|n| n.writable).map(|n| n.bond.clone())).context("receipt_bond_required")?;
            for frozen in &job.frozen {
                let index = snapshot.networks.iter().position(|n| n.writable && n.network.network_id == frozen.network_id);
                if frozen.deleted {
                    if let Some(i) = index { snapshot.networks.remove(i); }
                    continue;
                }
                let mut network = frozen.clone();
                if let Some(fields) = job.fields.get(&frozen.network_id) {
                    let old = index.map(|i| &snapshot.networks[i].network).context("new_network_requires_all_fields")?;
                    if !fields.iter().any(|f| f == "host_ssid") { network.host_ssid = old.host_ssid.clone(); }
                    if !fields.iter().any(|f| f == "ssid") { network.ssid = old.ssid.clone(); }
                    if !fields.iter().any(|f| f == "label") { network.label = old.label.clone(); }
                    if !fields.iter().any(|f| f == "credential") { network.password = old.password.clone(); }
                }
                network.version = revision; network.credential = "device".into();
                let origin_bond = index.map(|i| snapshot.networks[i].bond.clone()).unwrap_or_else(|| own_bond.clone());
                let observed = ObservedNetwork { bond: origin_bond, writable: true, network };
                if let Some(i) = index { snapshot.networks[i] = observed; } else { snapshot.networks.push(observed); }
            }
            snapshot.revision = revision; snapshot.snapshot_id = job.request_id.clone(); snapshot.observed_at = now;
            let result = self.confirm_sync(job_id, snapshot, true, now)?;
            let current = self.jobs.iter_mut().find(|j| j.job_id == job_id).unwrap();
            current.last_stage = "committed_receipt".into(); current.confirmation = Some("durable_receipt".into()); current.receipt_ack_pending=current.request_id.starts_with("w3_");
            let _ = result; return Ok(current.clone());
        }
        for frozen in &job.frozen {
            self.baseline.insert(frozen.network_id.clone(), frozen.metadata());
            if self.local_revision == job.local_revision {
                if let Some(target) = self.targets.iter_mut().find(|n| n == &frozen) {
                    target.version = revision; target.credential = "keep".into();
                }
            }
        }
        let j = self.jobs.iter_mut().find(|j| j.job_id == job_id).unwrap();
        j.state = "completed".into(); j.updated_at = now; j.last_stage = "committed_receipt".into();
        j.confirmed_revision = Some(revision); j.confirmation = Some("durable_receipt".into()); j.receipt_ack_pending=j.request_id.starts_with("w3_"); j.error_code = None;
        Ok(j.clone())
    }
    pub fn import(&mut self, selected: &[String], expected: u64, now: u64) -> Result<()> {
        self.import_fields(selected, &BTreeMap::new(), expected, now)
    }
    pub fn import_fields(&mut self, selected: &[String], fields: &BTreeMap<String, Vec<String>>, expected: u64, now: u64) -> Result<()> {
        if selected.is_empty() || expected != self.local_revision { bail!("local_revision_conflict"); }
        if fields.iter().any(|(id,fs)| !selected.contains(id) || fs.is_empty() || fs.iter().any(|f|
            !matches!(f.as_str(),"host_ssid"|"ssid"|"label"|"deleted"))) { bail!("invalid_wifi_fields"); }
        let s = self.snapshot.as_ref().ok_or_else(|| anyhow::anyhow!("snapshot_required"))?;
        let mut targets = self.targets.clone(); let mut baseline = self.baseline.clone();
        for id in selected {
            // Prefer the current target version; a foreign source never shadows it.
            let n = s.preferred_network(id)?;
            if !targets.iter().any(|t| &t.network_id == id) && fields.get(id).is_some_and(|fs|
                ["host_ssid","ssid","label","deleted"].iter().any(|f|!fs.iter().any(|s| s==f))) {
                bail!("new_network_requires_all_fields");
            }
            let mut t = n.network.clone(); t.credential = "device".into();
            if let (Some(fs), Some(old)) = (fields.get(id), targets.iter().find(|t| &t.network_id == id)) {
                t = old.clone();
                for f in fs {
                    match f.as_str() {
                        "ssid" => t.ssid = n.network.ssid.clone(), "host_ssid" => t.host_ssid = n.network.host_ssid.clone(),
                        "label" => t.label = n.network.label.clone(), "deleted" => t.deleted = false,
                        _ => bail!("invalid_wifi_fields"),
                    }
                }
            }
            t.origin = "local".into();
            if let Some(old) = targets.iter_mut().find(|t| &t.network_id == id) { *old = t; } else { targets.push(t); }
            let meta = n.network.metadata();
            if let Some(fs) = fields.get(id) {
                let base = baseline.entry(id.clone()).or_insert(json!({}));
                for f in fs { base[f] = meta[f].clone(); }
            } else { baseline.insert(id.clone(), meta); }
        }
        self.save(expected, targets, now)?; self.baseline = baseline;
        Ok(())
    }
    pub fn differences(&self) -> Vec<Value> {
        let mut ids: BTreeSet<_> = self.targets.iter().map(|n| n.network_id.clone()).collect();
        if let Some(s) = &self.snapshot { ids.extend(s.networks.iter().map(|n| n.network.network_id.clone())); }
        ids.into_iter().map(|id| {
            let local = self.targets.iter().find(|n| n.network_id == id);
            let remote: Vec<_> = self.snapshot.as_ref().map(|s| s.networks.iter().filter(|n| n.network.network_id == id).collect()).unwrap_or_default();
            let selected = self.snapshot.as_ref().and_then(|s|s.preferred_network(&id).ok());
            let actual = selected.map(|n| n.network.metadata());
            let target = local.map(Network::metadata);
            let base = self.baseline.get(&id);
            let fields: Vec<_> = ["host_ssid", "ssid", "label", "deleted"].iter().map(|f| {
                let l = target.as_ref().map(|v| v[*f].clone()).unwrap_or(Value::Null);
                let r = actual.as_ref().map(|v| v[*f].clone()).unwrap_or(Value::Null);
                let b = base.and_then(|v|v.get(*f)).cloned();
                let state = if l == r {"equal"} else if b.is_none() {"no_baseline"}
                    else if b.as_ref()==Some(&r) {"bridge_changed"} else if b.as_ref()==Some(&l) {"device_changed"} else {"conflict"};
                json!({"field":f,"bridge":l,"device":r,"baseline":b,"state":state,"conflict":state=="conflict"})
            }).collect();
            let password = match (local.and_then(|n| n.password.as_ref()), selected.and_then(|n| n.network.password.as_ref())) {
                (Some(a), Some(b)) if a == b => "equal",
                (Some(_), Some(_)) => "different",
                _ => "unknown",
            };
            let has=|kind:&str| fields.iter().any(|f|f["state"]==kind);
            let state = if self.snapshot.is_none() { "unknown_snapshot" }
                else if remote.len() > 1 && selected.is_none() { "ambiguous" }
                else if local.is_some_and(|n| n.deleted) { "explicit_delete" }
                else if local.is_none() { "device_only" }
                else if actual.is_none() { "bridge_only" }
                else if has("conflict") || password == "different" { "conflict" }
                else if has("no_baseline") { "no_baseline" }
                else if has("bridge_changed")&&has("device_changed") {"both_changed"}
                else if has("bridge_changed") {"bridge_changed"}else if has("device_changed"){"device_changed"}else{"equal"};
            json!({"network_id":id,"state":state,"bridge":local,"device":remote,"fields":fields,
                "writable":selected.map_or(remote.is_empty(),|n|n.writable),"password":password})
        }).collect()
    }
}

#[cfg(test)] mod connection_facts_tests {
    use super::*;
    fn facts() -> ConnectionFacts {
        let target:Network=serde_json::from_value(json!({"network_id":"home","host_ssid":"Host","ssid":"Device"})).unwrap();
        let mut c=ConnectionFacts::new(&target,1);
        c.observation=Some(NetworkObservation{boot_id:"boot-a".into(),at:100,state:"connected".into(),error:None,reason:0,ssid:Some("Device".into()),ip:Some("192.168.1.2".into())});c
    }
    #[test] fn contact_errors_do_not_rewrite_network_evidence_and_boot_change_invalidates_it() {
        let mut c=facts();c.note_http("boot-a".into(),101,Some("unauthorized".into()));
        let view=c.view(101,&[]);assert_eq!(view.state,"connected");assert_eq!(view.bridge_error.as_deref(),Some("unauthorized"));assert!(!view.http_ready);
        c.note_http("boot-b".into(),102,None);assert!(c.observation.is_none());assert!(c.view(102,&[]).http_ready);
        assert!(!c.view(463,&[]).http_ready);assert_eq!(c.view(463,&[]).state,"waiting_for_window");
    }
    #[test] fn newer_device_failure_supersedes_old_http_success() {
        let mut c=facts();c.note_http("boot-a".into(),100,None);
        assert!(c.view(100,&[]).http_ready);
        let o=c.observation.as_mut().unwrap();o.at=101;o.state="connection_failed".into();o.error=Some("connect_timeout".into());
        let view=c.view(101,&[]);assert!(!view.http_ready);assert_eq!(view.error.as_deref(),Some("connect_timeout"));
    }
    #[test] fn restart_keeps_intent_backoff_but_cannot_restore_online_evidence() {
        let mut state=WifiState::default();let mut c=facts();c.requested=true;c.attempts=2;c.next_attempt_at=180;c.note_http("boot-a".into(),101,None);state.connection_facts=Some(c);
        let bytes=serde_json::to_vec(&state).unwrap();let restarted:WifiState=serde_json::from_slice(&bytes).unwrap();
        let c=restarted.connection_facts.unwrap();assert!(c.requested);assert_eq!(c.attempts,2);assert_eq!(c.next_attempt_at,180);assert!(c.contact.is_none()&&c.observation.is_none());assert_eq!(c.view(101,&[]).state,"requested");
        let mut old=serde_json::to_value(&state).unwrap();old["connection"]=serde_json::to_value(facts().view(100,&[])).unwrap();
        let migrated:WifiState=serde_json::from_value(old.clone()).unwrap();assert_eq!(migrated.connection_facts.unwrap().view(100,&[]).state,"waiting_for_window");
        old["connection"]["error"]=json!("connect_timeout");
        let migrated:WifiState=serde_json::from_value(old.clone()).unwrap();assert!(migrated.connection_facts.unwrap().admission_error.is_none());
        old["connection"]["error"]=json!("authentication_failed");
        let migrated:WifiState=serde_json::from_value(old.clone()).unwrap();assert_eq!(migrated.connection_facts.unwrap().admission_error.as_deref(),Some("authentication_failed"));
        old["connection"]["schema"]=json!(3);assert!(serde_json::from_value::<WifiState>(old).is_err());
    }
}
