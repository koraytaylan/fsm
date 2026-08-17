//! Machine and instance store over the journal.

#![allow(clippy::collapsible_if, unused_imports)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use fsm_core::analyze::{EventStatus, enabled_events};
use fsm_core::canon::canon_bytes;
use fsm_core::error::{FsmError, retryable};
use fsm_core::expr::eval::{Budget, Val};
use fsm_core::hashes::{ResolveError, machine_id, resolve_machine_ref, state_hash};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{InstanceState, Status};
use fsm_core::record::{Record, RecordKind};
use fsm_core::replay::{NopSink, RecordSink, StoreState, StoredMachine, fold_with};
use fsm_core::spec::{Finding, TySpec, compile, parse_machine};
use fsm_core::step::{Outcome, Rejection, create, step, validate_event};
use fsm_core::tree::Tree;

use crate::journal_io::{self, Journal, JournalHealth, OpenError};

pub struct Store {
    pub journal: Journal,
    pub state: StoreState,
    pub history: BTreeMap<String, Vec<u64>>,
    pub records: Vec<Record>,
    pub data_dir: PathBuf,
    pub last_responses: BTreeMap<String, Value>,
    pub last_errors: BTreeMap<String, ErrorObj>,
}

struct HistSink {
    history: BTreeMap<String, Vec<u64>>,
    records: Vec<Record>,
}

impl RecordSink for HistSink {
    fn on_record(&mut self, record: &Record, _state: &StoreState) {
        self.records.push(record.clone());
        if let Some(iid) = record.body.get("instance_id").and_then(Value::as_str) {
            self.history.entry(iid.into()).or_default().push(record.seq);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorObj {
    pub code: String,
    pub message: String,
    pub path: String,
    pub span: Option<(u32, u32)>,
    pub source: Option<String>,
    pub hint: String,
    pub retryable: bool,
    pub details: Value,
    pub docs: String,
}

impl ErrorObj {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            code: code.into(),
            message: message.clone(),
            path: String::new(),
            span: None,
            source: None,
            hint: message,
            retryable: retryable(code),
            details: Value::Obj(BTreeMap::new()),
            docs: format!("fsm://docs/spec#{code}"),
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    pub fn details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    pub fn to_value(&self) -> Value {
        let mut m = BTreeMap::new();
        m.insert("code".into(), Value::Str(self.code.clone()));
        m.insert("message".into(), Value::Str(self.message.clone()));
        m.insert("path".into(), Value::Str(self.path.clone()));
        if let Some((s, e)) = self.span {
            let mut sp = BTreeMap::new();
            sp.insert("start".into(), Value::Num(s.to_string()));
            sp.insert("end".into(), Value::Num(e.to_string()));
            m.insert("span".into(), Value::Obj(sp));
        }
        m.insert("hint".into(), Value::Str(self.hint.clone()));
        m.insert("retryable".into(), Value::Bool(self.retryable));
        m.insert("details".into(), self.details.clone());
        m.insert("docs".into(), Value::Str(self.docs.clone()));
        Value::Obj(m)
    }

    pub fn from_fsm(e: FsmError) -> Self {
        Self {
            code: e.code,
            message: e.message,
            path: e.path,
            span: e.span,
            source: None,
            hint: e.hint,
            retryable: e.retryable,
            details: e.details,
            docs: e.docs,
        }
    }

    pub fn from_rejection(r: &Rejection) -> Self {
        Self::new(r.code, r.message.clone()).hint(r.hint.clone())
    }

    pub fn from_findings(fs: Vec<Finding>) -> Self {
        let first = fs.first().map(|f| f.code).unwrap_or("def/shape");
        let mut details = BTreeMap::new();
        details.insert(
            "findings".into(),
            Value::Arr(
                fs.iter()
                    .map(|f| {
                        let mut m = BTreeMap::new();
                        m.insert("code".into(), Value::Str(f.code.into()));
                        m.insert("message".into(), Value::Str(f.message.clone()));
                        m.insert("path".into(), Value::Str(f.path.clone()));
                        m.insert("hint".into(), Value::Str(f.hint.clone()));
                        Value::Obj(m)
                    })
                    .collect(),
            ),
        );
        let hint = fs.first().map(|f| f.hint.clone()).unwrap_or_default();
        let path = fs.first().map(|f| f.path.clone()).unwrap_or_default();
        Self::new(
            first,
            fs.iter().map(|f| f.code).collect::<Vec<_>>().join(","),
        )
        .hint(hint)
        .path(path)
        .details(Value::Obj(details))
    }
}

pub struct DefineOutcome {
    pub created: bool,
    pub machine_id: String,
    pub warnings: Vec<Finding>,
    pub name: String,
}

impl Store {
    pub fn open(data_dir: &Path) -> Result<Self, ErrorObj> {
        fs::create_dir_all(data_dir).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        let ver = data_dir.join("VERSION");
        if let Err(h) = journal_io::require_store_format(data_dir) {
            return Err(ErrorObj::new("store/version_mismatch", h.message())
                .hint("delete the data directory and recreate the store"));
        }
        if !ver.exists() {
            fs::write(&ver, "2\n").map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
            fs::create_dir_all(data_dir.join("snapshots")).ok();
        }
        let mut sink = HistSink {
            history: BTreeMap::new(),
            records: Vec::new(),
        };
        let (journal, state) = match journal_io::open(data_dir, &mut sink) {
            Ok(x) => x,
            Err(OpenError::Health(h)) => return Err(health_err(&h)),
            Err(OpenError::Io(s)) => return Err(ErrorObj::new("io/read", s)),
        };
        Ok(Store {
            journal,
            state,
            history: sink.history,
            records: sink.records,
            data_dir: data_dir.to_path_buf(),
            last_responses: BTreeMap::new(),
            last_errors: BTreeMap::new(),
        })
    }

    pub fn define_machine(
        &mut self,
        def: Value,
        dry_run: bool,
        if_exists_error: bool,
    ) -> Result<DefineOutcome, ErrorObj> {
        if fsm_core::canon::canon_bytes(&def).len() > fsm_core::limits::MAX_DEF_BYTES {
            return Err(ErrorObj::new(
                "def/limit_bytes",
                "definition exceeds 256 KiB",
            ));
        }
        let compiled = fsm_core::spec::compile_accepted(&def).map_err(ErrorObj::from_findings)?;
        let id = compiled.machine_id.clone();
        if machine_id(&def) != id {
            return Err(ErrorObj::new(
                "internal/identity",
                "compiled identity does not match accepted definition",
            ));
        }
        let name = compiled.spec.name.clone();
        let tree = Tree::build(&compiled.spec.states);
        let warnings = fsm_core::analyze::analyze_all(&compiled, &tree);
        if self.state.machines.contains_key(&id) {
            if if_exists_error {
                return Err(ErrorObj::new("req/machine_exists", id.clone())
                    .hint(format!("machine already stored as {id}")));
            }
            return Ok(DefineOutcome {
                created: false,
                machine_id: id,
                warnings,
                name,
            });
        }
        if dry_run {
            return Ok(DefineOutcome {
                created: true,
                machine_id: id,
                warnings,
                name,
            });
        }
        let mut body = BTreeMap::new();
        body.insert("machine_id".into(), Value::Str(id.clone()));
        body.insert("def".into(), def.clone());
        let rec = self
            .journal
            .append(RecordKind::MachineDefined, Value::Obj(body))
            .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        self.note_record(&rec);
        self.state.machines.insert(
            id.clone(),
            StoredMachine {
                def,
                compiled,
                tree,
            },
        );
        Ok(DefineOutcome {
            created: true,
            machine_id: id,
            warnings,
            name,
        })
    }

    pub fn resolve_machine(&self, reference: &str) -> Result<&StoredMachine, ErrorObj> {
        let ids = self.state.machines.keys().map(String::as_str);
        match resolve_machine_ref(ids, reference) {
            Ok(id) => self
                .state
                .machines
                .get(&id)
                .ok_or_else(|| ErrorObj::new("req/machine_not_found", reference)),
            Err(ResolveError::Ambiguous(v)) => {
                let mut details = BTreeMap::new();
                details.insert(
                    "candidates".into(),
                    Value::Arr(v.iter().cloned().map(Value::Str).collect()),
                );
                Err(ErrorObj::new("req/machine_ambiguous", reference)
                    .hint("use a full name@sha256:<64 hex> id")
                    .details(Value::Obj(details)))
            }
            Err(ResolveError::TooShort) => Err(ErrorObj::new(
                "req/machine_not_found",
                "hash prefix must be at least 12 hex digits",
            )
            .hint("use at least 12 hex digits")),
            Err(_) => {
                Err(ErrorObj::new("req/machine_not_found", reference)
                    .hint("machine add a spec first"))
            }
        }
    }

    fn replay_request(&self, request_id: &str) -> Option<Result<Value, ErrorObj>> {
        if let Some(mut r) = self.last_responses.get(request_id).cloned() {
            if let Value::Obj(o) = &mut r {
                o.insert("duplicate".into(), Value::Bool(true));
            }
            return Some(Ok(r));
        }
        if let Some(e) = self.last_errors.get(request_id) {
            return Some(Err(e.clone()));
        }
        if !self.state.dedup.contains_key(request_id) {
            return None;
        }
        let rec = self
            .records
            .iter()
            .rev()
            .find(|r| r.body.get("request_id").and_then(Value::as_str) == Some(request_id))?;
        if rec.kind == RecordKind::EventRejected {
            if let (Some(iid), Some(ev)) = (
                rec.body.get("instance_id").and_then(Value::as_str),
                rec.body.get("event").and_then(Value::as_str),
            ) {
                if let Ok(pre) = fold_prefix(&self.records, rec.seq.saturating_sub(1)) {
                    if let (Some(mid), Some(inst)) =
                        (pre.instance_machines.get(iid), pre.instances.get(iid))
                    {
                        if let Some(m) = pre.machines.get(mid) {
                            let payload = rec
                                .body
                                .get("payload")
                                .cloned()
                                .unwrap_or(Value::Obj(BTreeMap::new()));
                            let mut bud = Budget::new(4096);
                            if let Outcome::Rejected(r) =
                                step(&m.compiled, &m.tree, inst, ev, &payload, &mut bud)
                            {
                                let mut err = ErrorObj::from_rejection(&r);
                                if let Value::Obj(mut d) = err.details {
                                    d.insert("trace".into(), r.trace.to_value());
                                    err.details = Value::Obj(d);
                                }
                                return Some(Err(err));
                            }
                        }
                    }
                }
            }
        }
        if rec.kind == RecordKind::EventRejected || rec.kind == RecordKind::RequestRejected {
            let code = rec
                .body
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or("run/unhandled");
            let message = rec
                .body
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or(code);
            let hint = rec
                .body
                .get("hint")
                .and_then(Value::as_str)
                .unwrap_or(message);
            let mut err = ErrorObj::new(code, message).hint(hint);
            if let Some(d) = rec.body.get("details") {
                err.details = d.clone();
            }
            return Some(Err(err));
        }
        if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
            if rec.kind == RecordKind::EventApplied {
                if let Ok(pre) = fold_prefix(&self.records, rec.seq.saturating_sub(1)) {
                    if let Some(mut v) = reconstruct_applied(&pre, rec, iid, request_id) {
                        if let Value::Obj(o) = &mut v {
                            o.insert("duplicate".into(), Value::Bool(true));
                        }
                        return Some(Ok(v));
                    }
                }
            }
            if let Ok(folded) = fold_prefix(&self.records, rec.seq) {
                if let Ok(mut v) = view_at(&folded, iid, Some(request_id), Some(true), rec.seq) {
                    if let Value::Obj(o) = &mut v {
                        o.insert("duplicate".into(), Value::Bool(true));
                        if rec.kind == RecordKind::EventApplied {
                            o.insert("applied".into(), Value::Bool(true));
                        }
                        if rec.kind == RecordKind::EffectAcked {
                            if let Some(outc) = rec.body.get("outcome") {
                                o.insert("outcome".into(), outc.clone());
                            }
                            if let Some(res) = rec.body.get("result") {
                                o.insert("result".into(), res.clone());
                            }
                            o.insert("acked".into(), Value::Bool(true));
                        }
                    }
                    return Some(Ok(v));
                }
            }
        }
        Some(Ok(obj(&[("ok", "true"), ("duplicate", "true")])))
    }

    fn note_record(&mut self, rec: &Record) {
        self.records.push(rec.clone());
        self.state.last_seq = rec.seq;
        self.state.last_hash = rec.hash.clone();
    }

    pub fn allocate_request_id(&mut self) -> Result<String, ErrorObj> {
        let path = self.data_dir.join("alloc");
        let n = fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let next = n + 1;
        fs::write(&path, format!("{next}\n"))
            .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        Ok(format!("req-{}-{next}", crate::clock::now_ms()))
    }

    fn commit_dedup(&mut self, request_id: &str, resp: Value, seq: u64) {
        self.state.dedup.insert(request_id.into(), seq);
        self.last_responses.insert(request_id.into(), resp);
        self.state.last_seq = self.journal.last_seq;
        self.state.last_hash = self.journal.last_hash.clone();
    }

    pub fn create_instance(
        &mut self,
        machine_ref: &str,
        instance_id: &str,
        request_id: &str,
        expect_seq: Option<u64>,
    ) -> Result<Value, ErrorObj> {
        self.create_instance_ctx(
            machine_ref,
            instance_id,
            request_id,
            expect_seq,
            &BTreeMap::new(),
        )
    }

    pub fn create_instance_ctx(
        &mut self,
        machine_ref: &str,
        instance_id: &str,
        request_id: &str,
        expect_seq: Option<u64>,
        overrides: &BTreeMap<String, Val>,
    ) -> Result<Value, ErrorObj> {
        if let Some(r) = self.replay_request(request_id) {
            return r;
        }
        if let Some(exp) = expect_seq {
            if exp != self.journal.last_seq {
                return Err(ErrorObj::new(
                    "req/seq_mismatch",
                    "re-read the instance, then retry with the same request_id and the current seq",
                )
                .hint(
                    "re-read the instance, then retry with the same request_id and the current seq",
                ));
            }
        }
        let mid = {
            let m = self.resolve_machine(machine_ref)?;
            machine_id(&m.def)
        };
        let m = self
            .state
            .machines
            .get(&mid)
            .ok_or_else(|| ErrorObj::new("req/machine_not_found", machine_ref))?;
        let a =
            create(&m.compiled, &m.tree, overrides).map_err(|r| ErrorObj::from_rejection(&r))?;
        let pending: Vec<String> = a
            .effects
            .iter()
            .map(|e| format!("{instance_id}/0/{}", e.k))
            .collect();
        let inst = InstanceState {
            status: a.status_after,
            leaf: a.leaf_after.clone(),
            ctx: a.ctx_after.clone(),
            history: a.history_after.clone(),
            pending: pending.clone(),
        };
        let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, &inst);
        let mut ov = BTreeMap::new();
        for (k, v) in overrides {
            ov.insert(k.clone(), Value::Str(v.canonical_string()));
        }
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("machine_id".into(), Value::Str(mid.clone()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("state_hash".into(), Value::Str(sh.clone()));
        body.insert("leaf".into(), Value::Str(inst.leaf.clone()));
        body.insert("overrides".into(), Value::Obj(ov));
        let rec = self
            .journal
            .append(RecordKind::InstanceCreated, Value::Obj(body))
            .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        self.state.instances.insert(instance_id.into(), inst);
        self.state.instance_machines.insert(instance_id.into(), mid);
        self.history
            .entry(instance_id.into())
            .or_default()
            .push(rec.seq);
        self.records.push(rec.clone());
        let resp = self.instance_view(instance_id, Some(request_id), Some(false))?;
        self.commit_dedup(request_id, resp.clone(), rec.seq);
        Ok(resp)
    }

    pub fn send_event(
        &mut self,
        instance_id: &str,
        event: &str,
        mut payload: Value,
        request_id: &str,
        expect_seq: Option<u64>,
    ) -> Result<Value, ErrorObj> {
        self.send_event_stamp(
            instance_id,
            event,
            &mut payload,
            request_id,
            expect_seq,
            None,
        )
    }

    pub fn send_event_stamp(
        &mut self,
        instance_id: &str,
        event: &str,
        payload: &mut Value,
        request_id: &str,
        expect_seq: Option<u64>,
        stamp: Option<&str>,
    ) -> Result<Value, ErrorObj> {
        if let Some(r) = self.replay_request(request_id) {
            return r;
        }
        if let Some(exp) = expect_seq {
            if exp != self.journal.last_seq {
                return Err(ErrorObj::new(
                    "req/seq_mismatch",
                    "re-read the instance, then retry with the same request_id and the current seq",
                )
                .hint(
                    "re-read the instance, then retry with the same request_id and the current seq",
                ));
            }
        }
        if let Some(field) = stamp {
            if let Value::Obj(o) = payload {
                if !o.contains_key(field) {
                    o.insert(field.into(), Value::Str(crate::clock::now_ms().to_string()));
                }
            }
        }
        let mid = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .ok_or_else(|| ErrorObj::new("req/instance_not_found", instance_id))?;
        let m = self
            .state
            .machines
            .get(&mid)
            .ok_or_else(|| ErrorObj::new("req/machine_not_found", &mid))?;
        let inst = self
            .state
            .instances
            .get(instance_id)
            .ok_or_else(|| ErrorObj::new("req/instance_not_found", instance_id))?;
        if let Err(r) = validate_event(&m.compiled, event, payload) {
            return Err(ErrorObj::from_rejection(&r));
        }
        let mut bud = Budget::new(4096);
        let out = step(&m.compiled, &m.tree, inst, event, payload, &mut bud);
        match out {
            Outcome::Applied(a) => {
                let mut pending = inst.pending.clone();
                pending.extend(
                    a.effects
                        .iter()
                        .map(|e| format!("{instance_id}/{}/{}", self.journal.last_seq + 1, e.k)),
                );
                let new = InstanceState {
                    status: a.status_after,
                    leaf: a.leaf_after.clone(),
                    ctx: a.ctx_after.clone(),
                    history: a.history_after.clone(),
                    pending,
                };
                let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, &new);
                let mut body = BTreeMap::new();
                body.insert("instance_id".into(), Value::Str(instance_id.into()));
                body.insert("event".into(), Value::Str(event.into()));
                body.insert("payload".into(), payload.clone());
                body.insert("request_id".into(), Value::Str(request_id.into()));
                body.insert("state_hash".into(), Value::Str(sh.clone()));
                body.insert("source_state".into(), Value::Str(a.source_state.clone()));
                body.insert(
                    "exited".into(),
                    Value::Arr(a.exited.iter().cloned().map(Value::Str).collect()),
                );
                body.insert(
                    "entered".into(),
                    Value::Arr(a.entered.iter().cloned().map(Value::Str).collect()),
                );
                let rec = self
                    .journal
                    .append(RecordKind::EventApplied, Value::Obj(body))
                    .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
                self.state.instances.insert(instance_id.into(), new);
                self.history
                    .entry(instance_id.into())
                    .or_default()
                    .push(rec.seq);
                self.records.push(rec.clone());
                let mut resp = self.instance_view(instance_id, Some(request_id), Some(false))?;
                if let Value::Obj(o) = &mut resp {
                    o.insert("applied".into(), Value::Bool(true));
                    o.insert("ok".into(), Value::Str("true".into()));
                    o.insert("leaf".into(), Value::Str(a.leaf_after.clone()));
                    let mut tr = BTreeMap::new();
                    tr.insert("source_state".into(), Value::Str(a.source_state.clone()));
                    tr.insert(
                        "transition_idx".into(),
                        Value::Num(a.transition_idx.to_string()),
                    );
                    tr.insert("internal".into(), Value::Bool(a.internal));
                    tr.insert(
                        "exited".into(),
                        Value::Arr(a.exited.iter().cloned().map(Value::Str).collect()),
                    );
                    tr.insert(
                        "entered".into(),
                        Value::Arr(a.entered.iter().cloned().map(Value::Str).collect()),
                    );
                    o.insert("transition".into(), Value::Obj(tr));
                    o.insert("trace".into(), a.trace.to_value());
                    o.insert(
                        "monitor_flags".into(),
                        Value::Arr(a.monitor_flags.iter().cloned().map(Value::Str).collect()),
                    );
                }
                self.commit_dedup(request_id, resp.clone(), rec.seq);
                Ok(resp)
            }
            Outcome::Rejected(r) => {
                let inst = self.state.instances.get(instance_id).unwrap().clone();
                let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, &inst);
                let mut body = BTreeMap::new();
                body.insert("instance_id".into(), Value::Str(instance_id.into()));
                body.insert("request_id".into(), Value::Str(request_id.into()));
                body.insert("event".into(), Value::Str(event.into()));
                body.insert("payload".into(), payload.clone());
                body.insert("state_hash".into(), Value::Str(sh));
                body.insert("code".into(), Value::Str(r.code.into()));
                body.insert("message".into(), Value::Str(r.message.clone()));
                body.insert("hint".into(), Value::Str(r.hint.clone()));
                let rec = self
                    .journal
                    .append(RecordKind::EventRejected, Value::Obj(body))
                    .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
                self.history
                    .entry(instance_id.into())
                    .or_default()
                    .push(rec.seq);
                let mut err = ErrorObj::from_rejection(&r);
                if let Ok(view) = self.instance_view(instance_id, Some(request_id), None) {
                    if let Value::Obj(v) = view {
                        if let Some(en) = v.get("enabled_events") {
                            if let Value::Obj(d) = &mut err.details {
                                d.insert("enabled_events".into(), en.clone());
                            }
                        }
                    }
                }
                err.details = match err.details {
                    Value::Obj(mut d) => {
                        d.insert("trace".into(), r.trace.to_value());
                        Value::Obj(d)
                    }
                    other => other,
                };
                self.note_record(&rec);
                self.state.dedup.insert(request_id.into(), rec.seq);
                self.last_errors.insert(request_id.into(), err.clone());
                Err(err)
            }
            Outcome::Ignored => {
                let inst = self.state.instances.get(instance_id).unwrap().clone();
                let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, &inst);
                let mut body = BTreeMap::new();
                body.insert("instance_id".into(), Value::Str(instance_id.into()));
                body.insert("request_id".into(), Value::Str(request_id.into()));
                body.insert("event".into(), Value::Str(event.into()));
                body.insert("payload".into(), payload.clone());
                body.insert("state_hash".into(), Value::Str(sh));
                let rec = self
                    .journal
                    .append(RecordKind::EventIgnored, Value::Obj(body))
                    .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
                self.records.push(rec.clone());
                let resp = obj(&[("ok", "true"), ("ignored", "true")]);
                self.commit_dedup(request_id, resp.clone(), rec.seq);
                Ok(resp)
            }
        }
    }

    pub fn ack_effect(
        &mut self,
        instance_id: &str,
        effect_id: &str,
        request_id: &str,
    ) -> Result<Value, ErrorObj> {
        self.ack_effect_outcome(instance_id, effect_id, request_id, "ok", None)
    }

    pub fn ack_effect_outcome(
        &mut self,
        instance_id: &str,
        effect_id: &str,
        request_id: &str,
        outcome: &str,
        result: Option<Value>,
    ) -> Result<Value, ErrorObj> {
        if let Some(r) = self.replay_request(request_id) {
            return r;
        }
        if outcome != "ok" && outcome != "failed" {
            return Err(ErrorObj::new(
                "req/args_invalid",
                "outcome must be ok or failed",
            ));
        }
        let inst = self
            .state
            .instances
            .get(instance_id)
            .ok_or_else(|| ErrorObj::new("req/instance_not_found", instance_id))?;
        if !inst.pending.iter().any(|p| p == effect_id) {
            let listed = inst.pending.clone();
            let mut body = BTreeMap::new();
            body.insert("request_id".into(), Value::Str(request_id.into()));
            body.insert("instance_id".into(), Value::Str(instance_id.into()));
            body.insert("code".into(), Value::Str("req/field_unknown".into()));
            body.insert("message".into(), Value::Str("unknown effect id".into()));
            body.insert(
                "hint".into(),
                Value::Str("use an id from effects_pending".into()),
            );
            let mut det = BTreeMap::new();
            det.insert(
                "pending".into(),
                Value::Arr(inst.pending.iter().cloned().map(Value::Str).collect()),
            );
            body.insert("details".into(), Value::Obj(det));
            let rec = self
                .journal
                .append(RecordKind::RequestRejected, Value::Obj(body))
                .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
            self.note_record(&rec);
            let mut details = BTreeMap::new();
            details.insert(
                "pending".into(),
                Value::Arr(listed.into_iter().map(Value::Str).collect()),
            );
            let err = ErrorObj::new("req/field_unknown", "unknown effect id")
                .hint("use an id from effects_pending")
                .details(Value::Obj(details));
            self.last_errors.insert(request_id.into(), err.clone());
            self.state.dedup.insert(request_id.into(), rec.seq);
            return Err(err);
        }
        let pending: Vec<String> = inst
            .pending
            .iter()
            .filter(|p| *p != effect_id)
            .cloned()
            .collect();
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("effect_id".into(), Value::Str(effect_id.into()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("outcome".into(), Value::Str(outcome.into()));
        if let Some(res) = result.clone() {
            body.insert("result".into(), res);
        }
        let rec = self
            .journal
            .append(RecordKind::EffectAcked, Value::Obj(body))
            .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        if let Some(live) = self.state.instances.get_mut(instance_id) {
            live.pending.clone_from(&pending);
        }
        self.note_record(&rec);
        self.history
            .entry(instance_id.into())
            .or_default()
            .push(rec.seq);
        let mut m = BTreeMap::new();
        m.insert("ok".into(), Value::Str("true".into()));
        m.insert("acked".into(), Value::Bool(true));
        m.insert("instance_id".into(), Value::Str(instance_id.into()));
        m.insert("effect_id".into(), Value::Str(effect_id.into()));
        m.insert("outcome".into(), Value::Str(outcome.into()));
        m.insert("duplicate".into(), Value::Bool(false));
        m.insert("seq".into(), Value::Num(rec.seq.to_string()));
        m.insert(
            "effects_pending".into(),
            Value::Arr(pending.into_iter().map(Value::Str).collect()),
        );
        if let Some(res) = result {
            m.insert("result".into(), res);
        }
        let resp = Value::Obj(m);
        self.commit_dedup(request_id, resp.clone(), rec.seq);
        Ok(resp)
    }

    pub fn cancel_instance(
        &mut self,
        instance_id: &str,
        request_id: &str,
    ) -> Result<Value, ErrorObj> {
        self.cancel_instance_reason(instance_id, request_id, "")
    }

    pub fn cancel_instance_reason(
        &mut self,
        instance_id: &str,
        request_id: &str,
        reason: &str,
    ) -> Result<Value, ErrorObj> {
        if let Some(r) = self.replay_request(request_id) {
            return r;
        }
        if !self.state.instances.contains_key(instance_id) {
            return Err(ErrorObj::new("req/instance_not_found", instance_id));
        }
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("reason".into(), Value::Str(reason.into()));
        let rec = self
            .journal
            .append(RecordKind::InstanceCancelled, Value::Obj(body))
            .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        if let Some(inst) = self.state.instances.get_mut(instance_id) {
            inst.status = Status::Cancelled;
        }
        self.note_record(&rec);
        self.history
            .entry(instance_id.into())
            .or_default()
            .push(rec.seq);
        let resp = self.instance_view(instance_id, Some(request_id), Some(false))?;
        self.commit_dedup(request_id, resp.clone(), rec.seq);
        Ok(resp)
    }

    pub fn annotate(
        &mut self,
        instance_id: &str,
        request_id: &str,
        note: &str,
    ) -> Result<Value, ErrorObj> {
        if let Some(r) = self.replay_request(request_id) {
            return r;
        }
        if !self.state.instances.contains_key(instance_id) {
            return Err(ErrorObj::new("req/instance_not_found", instance_id));
        }
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("note".into(), Value::Str(note.into()));
        let rec = self
            .journal
            .append(RecordKind::Annotated, Value::Obj(body))
            .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        self.records.push(rec.clone());
        self.history
            .entry(instance_id.into())
            .or_default()
            .push(rec.seq);
        let mut m = BTreeMap::new();
        m.insert("ok".into(), Value::Str("true".into()));
        m.insert("note".into(), Value::Str(note.into()));
        let resp = Value::Obj(m);
        self.commit_dedup(request_id, resp.clone(), rec.seq);
        Ok(resp)
    }

    pub fn instance_view(
        &self,
        instance_id: &str,
        request_id: Option<&str>,
        duplicate: Option<bool>,
    ) -> Result<Value, ErrorObj> {
        let inst = self
            .state
            .instances
            .get(instance_id)
            .ok_or_else(|| ErrorObj::new("req/instance_not_found", instance_id))?;
        let mid = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .unwrap_or_default();
        let stored = self.state.machines.get(&mid);
        let mut ctx = BTreeMap::new();
        for (k, v) in &inst.ctx {
            ctx.insert(k.clone(), val_json(v));
        }
        let mut m = BTreeMap::new();
        m.insert("instance_id".into(), Value::Str(instance_id.into()));
        m.insert("ok".into(), Value::Str("true".into()));
        m.insert("status".into(), Value::Str(inst.status.as_str().into()));
        m.insert("leaf".into(), Value::Str(inst.leaf.clone()));
        if let Some(st) = stored {
            m.insert("state".into(), Value::Str(st.tree.dotted_path(&inst.leaf)));
            m.insert(
                "configuration".into(),
                Value::Arr(
                    st.tree
                        .configuration(&inst.leaf)
                        .into_iter()
                        .map(Value::Str)
                        .collect(),
                ),
            );
            let mut bud = Budget::new(4096);
            let evs = enabled_events(&st.compiled, &st.tree, inst, &mut bud);
            m.insert("enabled_events".into(), enabled_json(&evs));
            let mut mac = BTreeMap::new();
            mac.insert("machine_id".into(), Value::Str(mid.clone()));
            mac.insert("name".into(), Value::Str(st.compiled.spec.name.clone()));
            m.insert("machine".into(), Value::Obj(mac));
        } else {
            m.insert("state".into(), Value::Str(inst.leaf.clone()));
            m.insert("configuration".into(), Value::Arr(vec![]));
            m.insert("enabled_events".into(), Value::Arr(vec![]));
        }
        m.insert("context".into(), Value::Obj(ctx));
        m.insert(
            "effects_pending".into(),
            Value::Arr(inst.pending.iter().cloned().map(Value::Str).collect()),
        );
        m.insert("seq".into(), Value::Num(self.journal.last_seq.to_string()));
        m.insert(
            "state_hash".into(),
            Value::Str(state_hash(&mid, instance_id, self.journal.last_seq, inst)),
        );
        if let Some(r) = request_id {
            m.insert("request_id".into(), Value::Str(r.into()));
        }
        if let Some(d) = duplicate {
            m.insert("duplicate".into(), Value::Bool(d));
        }
        Ok(Value::Obj(m))
    }

    pub fn maybe_snapshot(&self) -> Result<(), ErrorObj> {
        if self.journal.last_seq > 0 && self.journal.last_seq % 10_000 == 0 {
            self.shutdown_snapshot()?;
        }
        Ok(())
    }

    pub fn shutdown_snapshot(&self) -> Result<(), ErrorObj> {
        let seq = self.journal.last_seq;
        let snap_dir = self.data_dir.join("snapshots");
        fs::create_dir_all(&snap_dir).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        let mut machines = BTreeMap::new();
        for (id, m) in &self.state.machines {
            machines.insert(id.clone(), m.def.clone());
        }
        let mut instances = BTreeMap::new();
        for (id, inst) in &self.state.instances {
            let mut o = BTreeMap::new();
            o.insert("leaf".into(), Value::Str(inst.leaf.clone()));
            o.insert("status".into(), Value::Str(inst.status.as_str().into()));
            o.insert(
                "machine_id".into(),
                Value::Str(
                    self.state
                        .instance_machines
                        .get(id)
                        .cloned()
                        .unwrap_or_default(),
                ),
            );
            instances.insert(id.clone(), Value::Obj(o));
        }
        let mut body = BTreeMap::new();
        body.insert("format".into(), Value::Str("fsm.snapshot/1".into()));
        body.insert("seq".into(), Value::Num(seq.to_string()));
        body.insert(
            "last_hash".into(),
            Value::Str(self.journal.last_hash.clone()),
        );
        body.insert("machines".into(), Value::Obj(machines));
        body.insert("instances".into(), Value::Obj(instances));
        let mut tmp_body = body.clone();
        tmp_body.insert("snapshot_hash".into(), Value::Str(String::new()));
        let hex = fsm_core::sha256::to_hex(&fsm_core::hashes::domain_hash(
            "fsm:snapshot:1",
            &Value::Obj(tmp_body.clone()),
        ));
        body.insert("snapshot_hash".into(), Value::Str(format!("sha256:{hex}")));
        let bytes = fsm_core::canon::canon_bytes(&Value::Obj(body));
        let tmp = snap_dir.join(format!("snap-{seq}-{}.tmp", std::process::id()));
        fs::write(&tmp, &bytes).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        let f = fs::File::open(&tmp).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        f.sync_all()
            .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        let final_path = snap_dir.join(format!("snap-{seq}.json"));
        if final_path.exists() {
            let alt = snap_dir.join(format!("snap-{seq}-{}.json", std::process::id()));
            fs::rename(&tmp, &alt).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        } else {
            fs::rename(&tmp, &final_path).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        }
        Ok(())
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = self.shutdown_snapshot();
    }
}

fn reconstruct_applied(
    pre: &StoreState,
    rec: &Record,
    iid: &str,
    request_id: &str,
) -> Option<Value> {
    let ev = rec.body.get("event").and_then(Value::as_str)?;
    let payload = rec
        .body
        .get("payload")
        .cloned()
        .unwrap_or(Value::Obj(BTreeMap::new()));
    let mid = pre.instance_machines.get(iid)?;
    let m = pre.machines.get(mid)?;
    let inst = pre.instances.get(iid)?;
    let mut bud = Budget::new(4096);
    match step(&m.compiled, &m.tree, inst, ev, &payload, &mut bud) {
        Outcome::Applied(a) => {
            let mut v = view_at(pre, iid, Some(request_id), Some(true), rec.seq).ok()?;
            if let Value::Obj(o) = &mut v {
                o.insert("applied".into(), Value::Bool(true));
                o.insert("ok".into(), Value::Str("true".into()));
                o.insert("leaf".into(), Value::Str(a.leaf_after.clone()));
                o.insert(
                    "state".into(),
                    Value::Str(m.tree.dotted_path(&a.leaf_after)),
                );
                let mut tr = BTreeMap::new();
                tr.insert("source_state".into(), Value::Str(a.source_state.clone()));
                tr.insert(
                    "transition_idx".into(),
                    Value::Num(a.transition_idx.to_string()),
                );
                tr.insert("internal".into(), Value::Bool(a.internal));
                tr.insert(
                    "exited".into(),
                    Value::Arr(a.exited.iter().cloned().map(Value::Str).collect()),
                );
                tr.insert(
                    "entered".into(),
                    Value::Arr(a.entered.iter().cloned().map(Value::Str).collect()),
                );
                o.insert("transition".into(), Value::Obj(tr));
                o.insert("trace".into(), a.trace.to_value());
                o.insert(
                    "monitor_flags".into(),
                    Value::Arr(a.monitor_flags.iter().cloned().map(Value::Str).collect()),
                );
            }
            Some(v)
        }
        _ => None,
    }
}

fn fold_prefix(records: &[Record], through: u64) -> Result<StoreState, ErrorObj> {
    let recs: Vec<Record> = records
        .iter()
        .filter(|r| r.seq <= through)
        .cloned()
        .collect();
    fold_with(recs, &mut NopSink)
        .map_err(|e| ErrorObj::new("store/state_hash_mismatch", format!("{e:?}")))
}

fn view_at(
    state: &StoreState,
    instance_id: &str,
    request_id: Option<&str>,
    duplicate: Option<bool>,
    seq: u64,
) -> Result<Value, ErrorObj> {
    let inst = state
        .instances
        .get(instance_id)
        .ok_or_else(|| ErrorObj::new("req/instance_not_found", instance_id))?;
    let mid = state
        .instance_machines
        .get(instance_id)
        .ok_or_else(|| ErrorObj::new("req/instance_not_found", instance_id))?;
    let m = state
        .machines
        .get(mid)
        .ok_or_else(|| ErrorObj::new("req/machine_not_found", mid.as_str()))?;
    let mut bud = Budget::new(4096);
    let enabled = enabled_events(&m.compiled, &m.tree, inst, &mut bud);
    let mut ctx = BTreeMap::new();
    for (k, v) in &inst.ctx {
        ctx.insert(k.clone(), val_json(v));
    }
    let mut mobj = BTreeMap::new();
    mobj.insert("ok".into(), Value::Str("true".into()));
    mobj.insert("instance_id".into(), Value::Str(instance_id.into()));
    mobj.insert("leaf".into(), Value::Str(inst.leaf.clone()));
    mobj.insert("state".into(), Value::Str(m.tree.dotted_path(&inst.leaf)));
    mobj.insert(
        "configuration".into(),
        Value::Arr(
            m.tree
                .configuration(&inst.leaf)
                .into_iter()
                .map(Value::Str)
                .collect(),
        ),
    );
    let mut mac = BTreeMap::new();
    mac.insert("machine_id".into(), Value::Str(mid.clone()));
    mac.insert("name".into(), Value::Str(m.compiled.spec.name.clone()));
    mobj.insert("machine".into(), Value::Obj(mac));
    mobj.insert("status".into(), Value::Str(inst.status.as_str().into()));
    mobj.insert("context".into(), Value::Obj(ctx));
    mobj.insert(
        "effects_pending".into(),
        Value::Arr(inst.pending.iter().cloned().map(Value::Str).collect()),
    );
    mobj.insert("seq".into(), Value::Num(seq.to_string()));
    mobj.insert(
        "state_hash".into(),
        Value::Str(state_hash(mid, instance_id, seq, inst)),
    );
    mobj.insert("enabled_events".into(), enabled_json(&enabled));
    if let Some(r) = request_id {
        mobj.insert("request_id".into(), Value::Str(r.into()));
    }
    if let Some(d) = duplicate {
        mobj.insert("duplicate".into(), Value::Bool(d));
    }
    Ok(Value::Obj(mobj))
}

fn health_err(h: &JournalHealth) -> ErrorObj {
    let code = match h {
        JournalHealth::TornTail { .. } => "store/torn_tail",
        JournalHealth::ChainBroken { .. } => "store/chain_broken",
        JournalHealth::StateHashMismatch { .. } => "store/state_hash_mismatch",
        JournalHealth::NonCanonical { .. } => "store/non_canonical",
        JournalHealth::LockIo(_) => "store/lock",
        JournalHealth::ReplayMismatch { .. } => "store/state_hash_mismatch",
        JournalHealth::MissingGenesis => "store/chain_broken",
        JournalHealth::Ok => "store/lock",
    };
    ErrorObj::new(code, h.message())
}

pub fn val_json(v: &Val) -> Value {
    match v {
        Val::Bool(b) => Value::Bool(*b),
        other => Value::Str(other.canonical_string()),
    }
}

pub fn enabled_json(evs: &[fsm_core::analyze::EventReport]) -> Value {
    Value::Arr(
        evs.iter()
            .map(|e| {
                let mut m = BTreeMap::new();
                m.insert("event".into(), Value::Str(e.event.clone()));
                m.insert(
                    "status".into(),
                    Value::Str(
                        match e.status {
                            EventStatus::Enabled => "enabled",
                            EventStatus::Disabled => "disabled",
                            EventStatus::DependsOnPayload => "depends_on_payload",
                            EventStatus::Preempted => "preempted",
                            EventStatus::PreemptedMaybe => "preempted_maybe",
                        }
                        .into(),
                    ),
                );
                if !e.payload_fields.is_empty() {
                    m.insert(
                        "payload_fields".into(),
                        Value::Arr(e.payload_fields.iter().cloned().map(Value::Str).collect()),
                    );
                }
                Value::Obj(m)
            })
            .collect(),
    )
}

pub fn coerce_ctx_override(ty: &TySpec, key: &str, raw: &str) -> Result<Val, ErrorObj> {
    match ty {
        TySpec::Bool => match raw {
            "true" => Ok(Val::Bool(true)),
            "false" => Ok(Val::Bool(false)),
            _ => Err(ErrorObj::new("req/field_type", key)),
        },
        TySpec::Int => raw
            .parse::<i64>()
            .map(Val::Int)
            .map_err(|_| ErrorObj::new("req/field_type", key)),
        TySpec::Str => Ok(Val::Str(raw.into())),
        TySpec::Ts => raw
            .parse::<i64>()
            .map(Val::Ts)
            .map_err(|_| ErrorObj::new("req/field_type", key)),
        TySpec::Dur => raw
            .parse::<i64>()
            .map(Val::Dur)
            .map_err(|_| ErrorObj::new("req/field_type", key)),
        TySpec::Dec { scale } => match fsm_core::decimal::Dec::parse(raw, *scale) {
            Ok(d) => Ok(Val::Dec(d)),
            Err(_) => {
                if raw.contains('.')
                    && raw.split('.').nth(1).map(|f| f.len()).unwrap_or(0) > *scale as usize
                {
                    Err(ErrorObj::new("req/field_scale", key)
                        .hint(format!("use exactly {scale} fraction digits")))
                } else {
                    Err(ErrorObj::new("req/field_type", key))
                }
            }
        },
        TySpec::Enum { of } => Ok(Val::Enum {
            ty: of.clone(),
            variant: raw.into(),
        }),
    }
}

fn obj(pairs: &[(&str, &str)]) -> Value {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert((*k).into(), Value::Str((*v).into()));
    }
    Value::Obj(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("fsm-s-{n}"));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn case_def() -> Value {
        parse(
            include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
            &JsonLimits::DEFAULT,
        )
        .unwrap()
    }

    #[test]
    fn define_idempotent_and_resolve() {
        let dir = tmp();
        let mut s = Store::open(&dir).unwrap();
        let d1 = s.define_machine(case_def(), false, false).unwrap();
        assert!(d1.created);
        let n = s.journal.last_seq;
        let d2 = s.define_machine(case_def(), false, false).unwrap();
        assert!(!d2.created);
        assert_eq!(d1.machine_id, d2.machine_id);
        assert_eq!(s.journal.last_seq, n);
        s.resolve_machine(&d1.machine_id).unwrap();
        s.resolve_machine("case_review").unwrap();
        let pref = format!(
            "case_review@sha256:{}",
            &d1.machine_id.split(':').next_back().unwrap()[..12]
        );
        s.resolve_machine(&pref).unwrap();
        assert!(dir.join("VERSION").exists());
        assert!(dir.join("journal").exists());
    }

    #[test]
    fn lost_response_retry_returns_original() {
        let dir = tmp();
        let mut s = Store::open(&dir).unwrap();
        s.define_machine(case_def(), false, false).unwrap();
        s.create_instance("case_review", "i1", "c1", None).unwrap();
        let seq = s.journal.last_seq;
        let r1 = s
            .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", Some(seq))
            .unwrap();
        let n = s.journal.last_seq;
        let r2 = s
            .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", Some(seq))
            .unwrap();
        assert_eq!(s.journal.last_seq, n);
        assert_eq!(r2.get("duplicate").and_then(Value::as_bool), Some(true));
        assert_eq!(r1.get("leaf"), r2.get("leaf"));
    }

    #[test]
    fn seq_mismatch_not_consumed() {
        let dir = tmp();
        let mut s = Store::open(&dir).unwrap();
        s.define_machine(case_def(), false, false).unwrap();
        s.create_instance("case_review", "i1", "c1", None).unwrap();
        let n = s.journal.last_seq;
        let err = s
            .send_event(
                "i1",
                "docs_ok",
                Value::Obj(BTreeMap::new()),
                "fresh",
                Some(0),
            )
            .unwrap_err();
        assert_eq!(err.code, "req/seq_mismatch");
        assert_eq!(s.journal.last_seq, n);
        s.send_event(
            "i1",
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            "fresh",
            Some(n),
        )
        .unwrap();
    }
}
