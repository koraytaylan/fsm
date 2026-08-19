//! Machine and instance store over the journal.

#![allow(clippy::collapsible_if, unused_imports)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use fsm_core::analyze::{EventStatus, enabled_events};
use fsm_core::canon::canon_bytes;
use fsm_core::error::{FsmError, retryable};
use fsm_core::expr::eval::{Budget, Val};
use fsm_core::hashes::{
    ResolveError, STATE_FORMAT, configuration_value, machine_id, resolve_machine_ref, state_hash,
};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{ActiveConfiguration, InstanceState, Status};
use fsm_core::record::{Record, RecordKind};
use fsm_core::replay::{
    NopSink, RecordSink, STATE_ROOT_FORMAT, StoreState, StoredMachine, ctx_val_json, fold_with,
};
use fsm_core::spec::{Finding, MachineSpec, TySpec};
use fsm_core::step::{
    DeadlineOutcome, Outcome, Rejection, create, poll_deadline, step, validate_event,
};
use fsm_core::tree::Tree;

use crate::journal_io::{self, Journal, JournalHealth, JournalIoError, OpenError};

pub struct Store {
    pub journal: Journal,
    pub state: StoreState,
    pub history: BTreeMap<String, Vec<u64>>,
    pub records: Vec<Record>,
    pub data_dir: PathBuf,
    pub last_responses: BTreeMap<String, Value>,
    pub last_errors: BTreeMap<String, ErrorObj>,
    pub tags: BTreeMap<String, Vec<String>>,
    pub replayed_records: usize,
    pub opened_from_snapshot: bool,
    pub opened_snapshot_seq: Option<u64>,
    /// Fingerprint of the request currently being committed. Set by
    /// `claim_request`, stamped into every record that request appends, and
    /// cleared on commit. Keeping it here rather than threading it through
    /// each body-building site means a new operation cannot forget it.
    pending_fp: Option<String>,
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
    pub duplicate: bool,
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
            duplicate: false,
        }
    }

    pub fn mark_duplicate(mut self) -> Self {
        self.duplicate = true;
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    pub fn request_id(mut self, rid: &str) -> Self {
        if let Value::Obj(d) = &mut self.details {
            d.insert("request_id".into(), Value::Str(rid.into()));
        }
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
        m.insert("duplicate".into(), Value::Bool(self.duplicate));
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
            duplicate: false,
        }
    }

    pub fn from_rejection(r: &Rejection) -> Self {
        let mut e = Self::new(r.code, r.message.clone()).hint(r.hint.clone());
        e.span = r.span;
        if let Value::Obj(d) = &mut e.details {
            if let Some(b) = &r.block {
                d.insert("block".into(), Value::Str(b.clone()));
            }
            if let Some(c) = r.cause {
                d.insert("cause".into(), Value::Str(c.into()));
            }
            if let Some(s) = r.source_state.as_ref() {
                d.insert("source_state".into(), Value::Str(s.clone()));
            }
            if let Some(idx) = r.transition_idx {
                d.insert("transition_idx".into(), Value::Num(idx.to_string()));
            }
            d.insert("trace".into(), r.trace.to_value());
        }
        e
    }

    pub fn with_store_catalog(mut self, store: &Store) -> Self {
        if let Value::Obj(d) = &mut self.details {
            let machines: Vec<Value> = store
                .state
                .machines
                .keys()
                .cloned()
                .map(Value::Str)
                .collect();
            let instances: Vec<Value> = store
                .state
                .instances
                .keys()
                .cloned()
                .map(Value::Str)
                .collect();
            if !machines.is_empty() {
                d.insert("known_machines".into(), Value::Arr(machines));
            }
            if !instances.is_empty() {
                d.insert("known_instances".into(), Value::Arr(instances));
            }
        }
        self
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
        let snapshot_directory = data_dir.join("snapshots");
        crate::persistence_directory_exists(&snapshot_directory)
            .map_err(|error| ErrorObj::new("io/write", error.to_string()))?;
        let mut sink = HistSink {
            history: BTreeMap::new(),
            records: Vec::new(),
        };
        let (journal, state, open_path) = match journal_io::open(data_dir, &mut sink) {
            Ok(x) => x,
            Err(OpenError::Health(h)) => return Err(health_err(&h)),
            Err(OpenError::ReadIo(message)) => {
                return Err(ErrorObj::new("io/read", message));
            }
            Err(OpenError::WriteIo(message)) => {
                return Err(ErrorObj::new("io/write", message));
            }
        };
        crate::ensure_persistence_directory(&snapshot_directory)
            .map_err(|error| ErrorObj::new("io/write", error.to_string()))?;
        let records = journal_io::load_records(data_dir).unwrap_or(sink.records);
        let mut history: BTreeMap<String, Vec<u64>> = BTreeMap::new();
        for rec in &records {
            if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                history.entry(iid.into()).or_default().push(rec.seq);
            }
        }
        let tags = load_tags_from_records(&records);
        Ok(Store {
            journal,
            state,
            history,
            records,
            data_dir: data_dir.to_path_buf(),
            last_responses: BTreeMap::new(),
            last_errors: BTreeMap::new(),
            tags,
            replayed_records: open_path.replayed_records,
            opened_from_snapshot: open_path.used_snapshot,
            opened_snapshot_seq: open_path.snapshot_seq,
            pending_fp: None,
        })
    }

    /// Load one internally consistent journal prefix for inspection without
    /// creating the data directory, taking the writer lock, migrating
    /// `VERSION`, or enabling snapshot writes.
    ///
    /// A live writer may append after this read; reopen to observe that later
    /// prefix. An unterminated line at the end of the final segment is omitted
    /// as an in-progress append; strict open and verification still report it
    /// as a torn tail. Mutating methods on the returned store fail with
    /// `io/write`.
    pub fn open_read_only(data_dir: &Path) -> Result<Self, ErrorObj> {
        let mut sink = HistSink {
            history: BTreeMap::new(),
            records: Vec::new(),
        };
        // `open_read_only` returns the exact record vector it folded. Loading
        // again here would let a live writer append between reads and produce
        // state from one prefix with history/tags from another.
        let (journal, state, open_path, records) =
            match journal_io::open_read_only(data_dir, &mut sink) {
                Ok(value) => value,
                Err(OpenError::Health(health)) => return Err(health_err(&health)),
                Err(OpenError::ReadIo(message) | OpenError::WriteIo(message)) => {
                    return Err(ErrorObj::new("io/read", message));
                }
            };
        let mut history: BTreeMap<String, Vec<u64>> = BTreeMap::new();
        for record in &records {
            if let Some(instance_id) = record.body.get("instance_id").and_then(Value::as_str) {
                history
                    .entry(instance_id.into())
                    .or_default()
                    .push(record.seq);
            }
        }
        let tags = load_tags_from_records(&records);
        Ok(Store {
            journal,
            state,
            history,
            records,
            data_dir: data_dir.to_path_buf(),
            last_responses: BTreeMap::new(),
            last_errors: BTreeMap::new(),
            tags,
            replayed_records: open_path.replayed_records,
            opened_from_snapshot: open_path.used_snapshot,
            opened_snapshot_seq: open_path.snapshot_seq,
            pending_fp: None,
        })
    }

    pub fn open_memory() -> Result<Self, ErrorObj> {
        let journal = Journal::memory();
        let records = journal.memory_records().unwrap_or(&[]).to_vec();
        let replayed_records = records.len();
        let state = fold_with(records.clone(), &mut NopSink)
            .map_err(|e| ErrorObj::new("store/state_hash_mismatch", format!("{e:?}")))?;
        Ok(Store {
            journal,
            state,
            history: BTreeMap::new(),
            records,
            data_dir: PathBuf::from("<memory>"),
            last_responses: BTreeMap::new(),
            last_errors: BTreeMap::new(),
            tags: BTreeMap::new(),
            replayed_records,
            opened_from_snapshot: false,
            opened_snapshot_seq: None,
            pending_fp: None,
        })
    }

    pub fn define_machine(
        &mut self,
        def: Value,
        dry_run: bool,
        if_exists_error: bool,
    ) -> Result<DefineOutcome, ErrorObj> {
        self.define_machine_on(
            &mut crate::clock::GlobalClock,
            def,
            dry_run,
            if_exists_error,
        )
    }

    pub fn define_machine_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        def: Value,
        dry_run: bool,
        if_exists_error: bool,
    ) -> Result<DefineOutcome, ErrorObj> {
        if !dry_run {
            self.ensure_writable()?;
        }
        if fsm_core::canon::canon_bytes(&def).len() > fsm_core::limits::MAX_DEF_BYTES {
            return Err(ErrorObj::new(
                "def/limit_bytes",
                "definition exceeds 256 KiB",
            ));
        }
        // Content identity is enough to recognize an immutable definition
        // that this journal already authenticated. In particular, a migrated
        // legacy definition may exceed a ceiling introduced later; requiring
        // current admission again would break the documented idempotent
        // `created: false` path even though no definition is being written.
        let candidate_id = machine_id(&def);
        if let Some(existing) = self
            .state
            .machines
            .get(&candidate_id)
            .filter(|existing| existing.def == def)
        {
            if if_exists_error {
                return Err(ErrorObj::new("req/machine_exists", candidate_id.clone())
                    .hint(format!("machine already stored as {candidate_id}")));
            }
            let mut warnings = fsm_core::analyze::analyze_all(&existing.compiled, &existing.tree);
            warnings.extend(existing.compiled.compile_warnings.clone());
            return Ok(DefineOutcome {
                created: false,
                machine_id: candidate_id,
                warnings,
                name: existing.compiled.spec.name.clone(),
            });
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
        let tree = Tree::for_machine(&compiled.spec);
        let mut warnings = fsm_core::analyze::analyze_all(&compiled, &tree);
        warnings.extend(compiled.compile_warnings.clone());
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
        let rec = self.append_rec(RecordKind::MachineDefined, Value::Obj(body), clock)?;
        self.note_record(&rec);
        self.state.machines.insert(
            id.clone(),
            StoredMachine {
                def,
                compiled,
                tree,
            },
        );
        self.finish_commit();
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
            Ok(id) => self.state.machines.get(&id).ok_or_else(|| {
                ErrorObj::new("req/machine_not_found", reference).with_store_catalog(self)
            }),
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
            .hint("use at least 12 hex digits")
            .with_store_catalog(self)),
            Err(_) => Err(ErrorObj::new("req/machine_not_found", reference)
                .hint("use a known machine id from details.known_machines")
                .with_store_catalog(self)),
        }
    }

    /// Look up a `request_id` against the idempotency ledger without claiming
    /// an unused key.
    ///
    /// `Ok(None)` means the key is unclaimed. `Ok(Some(_))` is the original
    /// outcome, replayed. `Err` is either the original error replayed, or
    /// `req/request_id_conflict` when the key was claimed by a *different*
    /// request.
    #[allow(clippy::type_complexity)]
    fn lookup_request(
        &mut self,
        request_id: &str,
        fp: &str,
    ) -> Result<Option<Result<Value, ErrorObj>>, ErrorObj> {
        self.pending_fp = None;
        if let Some(slot) = self.state.dedup.get(request_id) {
            if let Some(prev) = slot.fp.as_deref() {
                if prev != fp {
                    let claimed_seq = slot.seq;
                    return Err(self.request_id_conflict(request_id, claimed_seq));
                }
            }
        }
        Ok(self.replay_request(request_id))
    }

    /// Resolve a `request_id` against the idempotency ledger and stage its
    /// fingerprint for the records the operation will append.
    ///
    /// `Ok(None)` means the key is unclaimed and the caller should proceed;
    /// the fingerprint is stashed for the records the operation will append.
    /// `Ok(Some(_))` is the original outcome, replayed. `Err` is either the
    /// original error replayed, or `req/request_id_conflict` when the key was
    /// claimed by a *different* request — a reuse must never return an
    /// unrelated outcome as though it were this request's.
    #[allow(clippy::type_complexity)]
    fn claim_request(
        &mut self,
        request_id: &str,
        fp: String,
    ) -> Result<Option<Result<Value, ErrorObj>>, ErrorObj> {
        let replay = self.lookup_request(request_id, &fp)?;
        if replay.is_none() {
            self.pending_fp = Some(fp);
        }
        Ok(replay)
    }

    /// The key is taken by different content. Point at the record that claimed
    /// it so the caller can see what they actually sent, and say plainly that
    /// the remedy is a new key — not a retry.
    fn request_id_conflict(&self, request_id: &str, claimed_seq: u64) -> ErrorObj {
        let mut d = BTreeMap::new();
        d.insert("claimed_by_seq".into(), Value::Num(claimed_seq.to_string()));
        if let Some(rec) = self.records.iter().find(|r| r.seq == claimed_seq) {
            d.insert(
                "claimed_by".into(),
                Value::Str(rec.kind.as_str().to_string()),
            );
            for field in ["instance_id", "event", "deadline", "effect_id"] {
                if let Some(v) = rec.body.get(field) {
                    d.insert(format!("original_{field}"), v.clone());
                }
            }
        }
        ErrorObj::new(
            "req/request_id_conflict",
            "request_id already used for a different request",
        )
        .hint("this request_id is an idempotency key held by different content; send this request with a NEW request_id, and retry the original one only to replay its outcome")
        .details(Value::Obj(d))
        .request_id(request_id)
    }

    fn replay_request(&self, request_id: &str) -> Option<Result<Value, ErrorObj>> {
        if let Some(mut r) = self.last_responses.get(request_id).cloned() {
            if let Value::Obj(o) = &mut r {
                o.insert("duplicate".into(), Value::Bool(true));
            }
            return Some(Ok(r));
        }
        if let Some(e) = self.last_errors.get(request_id) {
            return Some(Err(e.clone().mark_duplicate()));
        }
        if !self.state.dedup.contains_key(request_id) {
            return None;
        }
        let rec = self
            .records
            .iter()
            .rev()
            .find(|r| r.body.get("request_id").and_then(Value::as_str) == Some(request_id))?;
        if matches!(
            rec.kind,
            RecordKind::EventRejected | RecordKind::DeadlineRejected | RecordKind::RequestRejected
        ) {
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
            let include_instance_identity = matches!(
                rec.kind,
                RecordKind::EventRejected | RecordKind::DeadlineRejected
            ) || (rec.kind == RecordKind::RequestRejected
                && rec.body.get("operation").and_then(Value::as_str) == Some("poll_deadline"));
            if include_instance_identity {
                if let Value::Obj(d) = &mut err.details {
                    if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                        d.insert("instance_id".into(), Value::Str(iid.into()));
                        if let Some(mid) = self.state.instance_machines.get(iid) {
                            d.insert("machine_id".into(), Value::Str(mid.clone()));
                        }
                    }
                }
            }
            if let Some(sp) = rec.body.get("span").and_then(Value::as_obj) {
                if let (Some(s), Some(e)) = (
                    sp.get("start")
                        .and_then(Value::as_num)
                        .and_then(|n| n.parse().ok()),
                    sp.get("end")
                        .and_then(Value::as_num)
                        .and_then(|n| n.parse().ok()),
                ) {
                    err.span = Some((s, e));
                }
            }
            return Some(Err(err.request_id(request_id).mark_duplicate()));
        }
        if rec.kind == RecordKind::EffectAcked {
            let mut m = BTreeMap::new();
            m.insert("ok".into(), Value::Str("true".into()));
            m.insert("acked".into(), Value::Bool(true));
            if let Some(v) = rec.body.get("instance_id") {
                m.insert("instance_id".into(), v.clone());
            }
            if let Some(v) = rec.body.get("effect_id") {
                m.insert("effect_id".into(), v.clone());
            }
            if let Some(v) = rec.body.get("outcome") {
                m.insert("outcome".into(), v.clone());
            }
            m.insert("request_id".into(), Value::Str(request_id.into()));
            if let Some(v) = rec.body.get("result") {
                m.insert("result".into(), v.clone());
            }
            m.insert("duplicate".into(), Value::Bool(true));
            m.insert("seq".into(), Value::Num(rec.seq.to_string()));
            if let Ok(folded) = fold_prefix(&self.records, rec.seq) {
                if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                    if let Some(inst) = folded.instances.get(iid) {
                        m.insert(
                            "effects_pending".into(),
                            Value::Arr(inst.pending.iter().cloned().map(Value::Str).collect()),
                        );
                    }
                }
            }
            return Some(Ok(Value::Obj(m)));
        }
        if rec.kind == RecordKind::Annotated {
            let mut m = BTreeMap::new();
            m.insert("ok".into(), Value::Str("true".into()));
            if let Some(n) = rec.body.get("note") {
                m.insert("note".into(), n.clone());
            }
            m.insert("request_id".into(), Value::Str(request_id.into()));
            m.insert("duplicate".into(), Value::Bool(true));
            return Some(Ok(Value::Obj(m)));
        }
        if rec.kind == RecordKind::EventIgnored {
            if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                if let Ok(folded) = fold_prefix(&self.records, rec.seq) {
                    if let Some(mut v) = reconstruct_ignored(&folded, rec, iid, request_id) {
                        if let Value::Obj(o) = &mut v {
                            o.insert("duplicate".into(), Value::Bool(true));
                        }
                        return Some(Ok(v));
                    }
                }
            }
        }
        if rec.kind == RecordKind::DeadlineNotDue {
            if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                if let Ok(folded) = fold_prefix(&self.records, rec.seq) {
                    if let Ok(mut value) =
                        view_at(&folded, iid, Some(request_id), Some(true), rec.seq)
                    {
                        if let Value::Obj(output) = &mut value {
                            output.insert("deadline_applied".into(), Value::Bool(false));
                            output.insert("deadline_not_due".into(), Value::Bool(true));
                            for field in ["next_deadline", "next_deadline_idx"] {
                                if let Some(field_value) = rec.body.get(field) {
                                    output.insert(field.into(), field_value.clone());
                                }
                            }
                            if let Some(next_due_ms) =
                                rec.body.get("next_due_ms").and_then(Value::as_num)
                            {
                                output.insert(
                                    "next_due_ms".into(),
                                    Value::Str(next_due_ms.to_string()),
                                );
                            }
                        }
                        return Some(Ok(value));
                    }
                }
            }
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
            if rec.kind == RecordKind::DeadlineApplied {
                if let Ok(pre) = fold_prefix(&self.records, rec.seq.saturating_sub(1)) {
                    if let Some(mut value) =
                        reconstruct_deadline_applied(&pre, rec, iid, request_id)
                    {
                        if let Value::Obj(output) = &mut value {
                            output.insert("duplicate".into(), Value::Bool(true));
                        }
                        return Some(Ok(value));
                    }
                }
            }
            if let Ok(folded) = fold_prefix(&self.records, rec.seq) {
                if let Ok(mut v) = view_at(&folded, iid, Some(request_id), Some(true), rec.seq) {
                    if let Value::Obj(o) = &mut v {
                        o.insert("duplicate".into(), Value::Bool(true));
                        if rec.kind == RecordKind::EventApplied {
                            o.insert("applied".into(), Value::Bool(true));
                        } else if rec.kind == RecordKind::DeadlineApplied {
                            o.insert("deadline_applied".into(), Value::Bool(true));
                        }
                    }
                    return Some(Ok(v));
                }
            }
        }
        Some(Ok(Value::Obj(BTreeMap::from([
            ("ok".into(), Value::Str("true".into())),
            ("duplicate".into(), Value::Bool(true)),
        ]))))
    }

    /// Fingerprint of a `create` request. The machine reference is fingerprinted
    /// as written, not as resolved: resolution happens after this check, and a
    /// retry resends the same text. A different spelling of the same machine is
    /// therefore a conflict — strict, but never silently wrong.
    fn fp_create(
        machine_ref: &str,
        instance_id: &str,
        overrides: &BTreeMap<String, Val>,
        tags: &[String],
    ) -> String {
        let ov = overrides
            .iter()
            .map(|(k, v)| (k.clone(), Value::Str(fsm_core::replay::ctx_val_string(v))))
            .collect();
        fsm_core::hashes::request_fp(
            "create",
            &BTreeMap::from([
                ("machine".into(), Value::Str(machine_ref.into())),
                ("instance_id".into(), Value::Str(instance_id.into())),
                ("overrides".into(), Value::Obj(ov)),
                (
                    "tags".into(),
                    Value::Arr(tags.iter().cloned().map(Value::Str).collect()),
                ),
            ]),
        )
    }

    /// Fingerprint of a `send` request, over the payload **as received** —
    /// before stamp fields are filled in, so an honest retry of a stamped send
    /// still matches.
    fn fp_send(instance_id: &str, event: &str, payload: &Value) -> String {
        fsm_core::hashes::request_fp(
            "send",
            &BTreeMap::from([
                ("instance_id".into(), Value::Str(instance_id.into())),
                ("event".into(), Value::Str(event.into())),
                ("payload".into(), payload.clone()),
            ]),
        )
    }

    fn fp_poll_deadline(instance_id: &str) -> String {
        fsm_core::hashes::request_fp(
            "poll_deadline",
            &BTreeMap::from([("instance_id".into(), Value::Str(instance_id.into()))]),
        )
    }

    fn fp_ack(instance_id: &str, effect_id: &str, outcome: &str, result: Option<&Value>) -> String {
        fsm_core::hashes::request_fp(
            "ack",
            &BTreeMap::from([
                ("instance_id".into(), Value::Str(instance_id.into())),
                ("effect_id".into(), Value::Str(effect_id.into())),
                ("outcome".into(), Value::Str(outcome.into())),
                ("result".into(), result.cloned().unwrap_or(Value::Null)),
            ]),
        )
    }

    fn fp_cancel(instance_id: &str, reason: &str) -> String {
        fsm_core::hashes::request_fp(
            "cancel",
            &BTreeMap::from([
                ("instance_id".into(), Value::Str(instance_id.into())),
                ("reason".into(), Value::Str(reason.into())),
            ]),
        )
    }

    fn fp_annotate(instance_id: &str, note: &str) -> String {
        fsm_core::hashes::request_fp(
            "annotate",
            &BTreeMap::from([
                ("instance_id".into(), Value::Str(instance_id.into())),
                ("note".into(), Value::Str(note.into())),
            ]),
        )
    }

    /// Reject a value too large to journal.
    ///
    /// Checked before the request is applied, so an oversized payload costs
    /// nothing but the error — it never reaches the journal, which would carry
    /// it forever.
    fn check_journalled_size(what: &str, v: &Value, request_id: &str) -> Result<(), ErrorObj> {
        let bytes = canon_bytes(v).len();
        if bytes <= fsm_core::limits::MAX_PAYLOAD_BYTES {
            return Ok(());
        }
        let max = fsm_core::limits::MAX_PAYLOAD_BYTES;
        let mut d = BTreeMap::new();
        d.insert("field".into(), Value::Str(what.into()));
        d.insert("bytes".into(), Value::Num(bytes.to_string()));
        d.insert("max_bytes".into(), Value::Num(max.to_string()));
        Err(ErrorObj::new(
            "req/payload_too_large",
            format!("{what} is {bytes} bytes; the limit is {max}"),
        )
        .hint(format!(
            "journal records are permanent: send a digest or an identifier in {what} and keep the payload in your own store"
        ))
        .details(Value::Obj(d))
        .request_id(request_id))
    }

    fn note_record(&mut self, rec: &Record) {
        self.records.push(rec.clone());
        self.state.last_seq = rec.seq;
        self.state.last_hash = rec.hash.clone();
    }

    fn finish_commit(&mut self) {
        self.pending_fp = None;
        self.after_commit();
    }

    /// Stamp the in-flight request's fingerprint onto any body that claims a
    /// `request_id`, so the fold can rebuild the idempotency ledger with the
    /// content each key was claimed for. Done in the one funnel every append
    /// passes through rather than at each body-building site.
    fn stamp_request_fp(&self, body: Value) -> Value {
        match (self.pending_fp.as_ref(), body) {
            (Some(fp), Value::Obj(mut o)) if o.contains_key("request_id") => {
                o.insert("request_fp".into(), Value::Str(fp.clone()));
                Value::Obj(o)
            }
            (_, other) => other,
        }
    }

    fn journal_write_error(error: JournalIoError, request_id: Option<&str>) -> ErrorObj {
        let mut output = match error {
            JournalIoError::RecordTooLarge { bytes, max_bytes } => {
                let details = Value::Obj(BTreeMap::from([
                    ("bytes".into(), Value::Num(bytes.to_string())),
                    ("max_bytes".into(), Value::Num(max_bytes.to_string())),
                ]));
                ErrorObj::new(
                    "io/write",
                    format!(
                        "journal record is {bytes} bytes; the limit is {max_bytes} bytes"
                    ),
                )
                .hint(
                    "shorten identifiers, creation overrides, tags, or cancellation reasons before retrying",
                )
                .details(details)
            }
            other => ErrorObj::new("io/write", other.to_string()),
        };
        if let Some(request_id) = request_id {
            output = output.request_id(request_id);
        }
        output
    }

    fn append_at_with_root(
        &mut self,
        kind: RecordKind,
        body: Value,
        ts: i64,
    ) -> Result<Record, ErrorObj> {
        let body = self.stamp_request_fp(body);
        let request_id = body
            .get("request_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let seq = self.journal.last_seq.saturating_add(1);
        if !seq.is_multiple_of(10_000) {
            return self
                .journal
                .append_at(kind, body, ts)
                .map_err(|error| Self::journal_write_error(error, request_id.as_deref()));
        }
        let mut body = body.as_obj().cloned().ok_or_else(|| {
            ErrorObj::new("store/state_hash_mismatch", "record body is not an object")
        })?;
        let provisional = fsm_core::record::seal(
            seq,
            ts,
            kind,
            Value::Obj(body.clone()),
            &self.journal.last_hash,
        );
        let projected = fsm_core::replay::fold_from(
            self.state.clone(),
            [provisional],
            &mut fsm_core::replay::NopSink,
        )
        .map_err(|e| ErrorObj::new("store/state_hash_mismatch", format!("{e:?}")))?;
        body.insert(
            "state_root".into(),
            Value::Str(fsm_core::replay::state_root_at(&projected, seq)),
        );
        body.insert(
            "state_root_format".into(),
            Value::Str(STATE_ROOT_FORMAT.into()),
        );
        self.journal
            .append_at(kind, Value::Obj(body), ts)
            .map_err(|error| Self::journal_write_error(error, request_id.as_deref()))
    }

    fn ensure_writable(&self) -> Result<(), ErrorObj> {
        if self.journal.is_read_only() {
            Err(ErrorObj::new("io/write", "store was opened read-only"))
        } else {
            Ok(())
        }
    }

    fn append_rec(
        &mut self,
        kind: RecordKind,
        body: Value,
        clock: &mut dyn crate::clock::Clock,
    ) -> Result<Record, ErrorObj> {
        let ts = clock.now_ms();
        self.append_at_with_root(kind, body, ts)
    }

    pub fn allocate_request_id(&mut self) -> Result<String, ErrorObj> {
        self.ensure_writable()?;
        let path = self.data_dir.join("alloc");
        let n = crate::read_regular_string_capped(&path, crate::PERSISTENCE_READ_CAP)
            .ok()
            .and_then(|s| {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    t.parse::<u64>().ok()
                }
            })
            .unwrap_or(self.journal.last_seq);
        let mut next = n
            .saturating_add(1)
            .max(self.journal.last_seq.saturating_add(1));
        loop {
            let cand = format!("req-{}-{next}", self.journal.last_seq);
            if !self.state.dedup.contains_key(&cand) {
                let tmp = self.data_dir.join("alloc.tmp");
                crate::write_durable(&tmp, format!("{next}\n").as_bytes())
                    .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
                fs::rename(&tmp, &path).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
                crate::sync_dir(&self.data_dir)
                    .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
                return Ok(cand);
            }
            next += 1;
        }
    }

    /// Record which request claimed the key, so a later reuse with different
    /// content is a conflict rather than a replay of this outcome.
    fn claimed_slot(&self, seq: u64) -> fsm_core::replay::RequestSlot {
        fsm_core::replay::RequestSlot {
            seq,
            fp: self.pending_fp.clone(),
        }
    }

    fn commit_dedup(&mut self, request_id: &str, resp: Value, seq: u64) {
        let slot = self.claimed_slot(seq);
        self.state.dedup.insert(request_id.into(), slot);
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
            &[],
        )
    }

    pub fn create_instance_ctx(
        &mut self,
        machine_ref: &str,
        instance_id: &str,
        request_id: &str,
        expect_seq: Option<u64>,
        overrides: &BTreeMap<String, Val>,
        tags: &[String],
    ) -> Result<Value, ErrorObj> {
        self.create_instance_ctx_on(
            &mut crate::clock::GlobalClock,
            machine_ref,
            instance_id,
            request_id,
            expect_seq,
            overrides,
            tags,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_instance_ctx_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        machine_ref: &str,
        instance_id: &str,
        request_id: &str,
        expect_seq: Option<u64>,
        overrides: &BTreeMap<String, Val>,
        tags: &[String],
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        if let Some(r) = self.claim_request(
            request_id,
            Self::fp_create(machine_ref, instance_id, overrides, tags),
        )? {
            return r;
        }
        if let Some(exp) = expect_seq {
            if exp != self.journal.last_seq {
                let mut d = BTreeMap::new();
                d.insert(
                    "current_seq".into(),
                    Value::Num(self.journal.last_seq.to_string()),
                );
                return Err(ErrorObj::new(
                    "req/seq_mismatch",
                    "re-read the instance, then retry with the same request_id and the current seq",
                )
                .hint(
                    "re-read the instance, then retry with the same request_id and the current seq",
                )
                .details(Value::Obj(d))
                .request_id(request_id)
                .with_store_catalog(self));
            }
        }
        let mid = {
            let m = self
                .resolve_machine(machine_ref)
                .map_err(|e| e.request_id(request_id))?;
            machine_id(&m.def)
        };
        let m = self.state.machines.get(&mid).ok_or_else(|| {
            ErrorObj::new("req/machine_not_found", machine_ref)
                .request_id(request_id)
                .with_store_catalog(self)
        })?;
        let commit_ts = clock.now_ms();
        let a = create(&m.compiled, &m.tree, overrides, commit_ts).map_err(|r| {
            let mut e = ErrorObj::from_rejection(&r)
                .request_id(request_id)
                .with_store_catalog(self);
            if let Value::Obj(d) = &mut e.details {
                d.insert("machine".into(), Value::Str(machine_ref.into()));
                d.insert("machine_id".into(), Value::Str(mid.clone()));
                d.insert(
                    "context_fields".into(),
                    Value::Arr(
                        m.compiled
                            .spec
                            .context
                            .iter()
                            .map(|c| {
                                Value::Obj(BTreeMap::from([
                                    ("name".into(), Value::Str(c.name.clone())),
                                    ("type".into(), Value::Str(c.ty.to_ty().to_string())),
                                    ("init".into(), Value::Str(c.init.clone())),
                                ]))
                            })
                            .collect(),
                    ),
                );
                let created: Vec<Value> = self
                    .state
                    .instance_machines
                    .values()
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .filter(|id| id != &mid)
                    .map(Value::Str)
                    .collect();
                if !created.is_empty() {
                    d.insert("known_machines".into(), Value::Arr(created));
                }
            }
            e
        })?;
        let pending: Vec<String> = a
            .effects
            .iter()
            .map(|e| format!("{instance_id}/0/{}", e.k))
            .collect();
        let inst = InstanceState {
            status: a.status_after,
            configuration: a.configuration_after.clone(),
            ctx: a.ctx_after.clone(),
            history: a.history_after.clone(),
            deadlines: a.deadlines_after.clone(),
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
        body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        body.insert(
            "configuration".into(),
            configuration_value(&inst.configuration),
        );
        body.insert("overrides".into(), Value::Obj(ov));
        if !tags.is_empty() {
            body.insert(
                "tags".into(),
                Value::Arr(tags.iter().cloned().map(Value::Str).collect()),
            );
        }
        let rec =
            self.append_at_with_root(RecordKind::InstanceCreated, Value::Obj(body), commit_ts)?;
        self.state.instances.insert(instance_id.into(), inst);
        self.state.instance_machines.insert(instance_id.into(), mid);
        if !tags.is_empty() {
            self.tags.insert(instance_id.into(), tags.to_vec());
        }
        self.history
            .entry(instance_id.into())
            .or_default()
            .push(rec.seq);
        self.note_record(&rec);
        let resp = self.instance_view(instance_id, Some(request_id), Some(false))?;
        self.commit_dedup(request_id, resp.clone(), rec.seq);
        self.finish_commit();
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
            &[],
        )
    }

    pub fn send_event_stamp(
        &mut self,
        instance_id: &str,
        event: &str,
        payload: &mut Value,
        request_id: &str,
        expect_seq: Option<u64>,
        stamps: &[&str],
    ) -> Result<Value, ErrorObj> {
        self.send_event_stamp_on(
            &mut crate::clock::GlobalClock,
            instance_id,
            event,
            payload,
            request_id,
            expect_seq,
            stamps,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn send_event_stamp_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        instance_id: &str,
        event: &str,
        payload: &mut Value,
        request_id: &str,
        expect_seq: Option<u64>,
        stamps: &[&str],
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        let request_fp = Self::fp_send(instance_id, event, payload);
        if let Some(r) = self.lookup_request(request_id, &request_fp)? {
            return r;
        }

        // Stamp a candidate rather than the caller's value so every
        // unjournaled rejection remains atomic. A single reservation supplies
        // every absent field and lets us enforce the cap against the exact
        // payload that will be journalled without advancing built-in clocks.
        let mut final_payload = payload.clone();
        let mut reserved_ts = None;
        if let Value::Obj(fields) = &mut final_payload {
            for field in stamps {
                if !fields.contains_key(*field) {
                    let timestamp = *reserved_ts.get_or_insert_with(|| clock.reserve_ms());
                    fields.insert((*field).into(), Value::Str(timestamp.to_string()));
                }
            }
        }
        Self::check_journalled_size("payload", &final_payload, request_id)?;
        if let Some(exp) = expect_seq {
            if exp != self.journal.last_seq {
                let mut d = BTreeMap::new();
                d.insert(
                    "current_seq".into(),
                    Value::Num(self.journal.last_seq.to_string()),
                );
                return Err(ErrorObj::new(
                    "req/seq_mismatch",
                    "re-read the instance, then retry with the same request_id and the current seq",
                )
                .hint(
                    "re-read the instance, then retry with the same request_id and the current seq",
                )
                .details(Value::Obj(d))
                .request_id(request_id)
                .with_store_catalog(self));
            }
        }
        let mid = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .ok_or_else(|| {
                ErrorObj::new("req/instance_not_found", instance_id)
                    .request_id(request_id)
                    .hint("use a known instance id from details.known_instances")
                    .with_store_catalog(self)
            })?;
        let m = self.state.machines.get(&mid).ok_or_else(|| {
            ErrorObj::new("req/machine_not_found", &mid)
                .request_id(request_id)
                .with_store_catalog(self)
        })?;
        let response_tree = m.tree.clone();
        let inst = self.state.instances.get(instance_id).ok_or_else(|| {
            ErrorObj::new("req/instance_not_found", instance_id)
                .request_id(request_id)
                .hint("use a known instance id from details.known_instances")
                .with_store_catalog(self)
        })?;
        let from_configuration = inst.configuration.clone();
        // SPEC step ordering status-gates before event validation. Request
        // shape errors remain unjournaled only while the instance is running;
        // completed/cancelled outcomes depend on durable instance state and
        // must flow through `step` so they are journaled and replayable.
        if inst.status == Status::Running {
            if let Err(r) = validate_event(&m.compiled, event, &final_payload) {
                return Err(ErrorObj::from_rejection(&r).request_id(request_id));
            }
        }
        let commit_ts = reserved_ts
            .map(|timestamp| clock.commit_reserved_ms(timestamp))
            .unwrap_or_else(|| clock.now_ms());
        self.pending_fp = Some(request_fp);
        *payload = final_payload;
        let mut bud = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
        let out = step(
            &m.compiled,
            &m.tree,
            inst,
            event,
            payload,
            commit_ts,
            &mut bud,
        );
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
                    configuration: a.configuration_after.clone(),
                    ctx: a.ctx_after.clone(),
                    history: a.history_after.clone(),
                    deadlines: a.deadlines_after.clone(),
                    pending,
                };
                let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, &new);
                let mut body = BTreeMap::new();
                body.insert("instance_id".into(), Value::Str(instance_id.into()));
                body.insert("event".into(), Value::Str(event.into()));
                body.insert("payload".into(), payload.clone());
                body.insert("request_id".into(), Value::Str(request_id.into()));
                body.insert("state_hash".into(), Value::Str(sh.clone()));
                body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
                body.insert("source_state".into(), Value::Str(a.source_state.clone()));
                body.insert(
                    "exited".into(),
                    Value::Arr(a.exited.iter().cloned().map(Value::Str).collect()),
                );
                body.insert(
                    "entered".into(),
                    Value::Arr(a.entered.iter().cloned().map(Value::Str).collect()),
                );
                let rec = self.append_at_with_root(
                    RecordKind::EventApplied,
                    Value::Obj(body),
                    commit_ts,
                )?;
                self.state.instances.insert(instance_id.into(), new);
                self.history
                    .entry(instance_id.into())
                    .or_default()
                    .push(rec.seq);
                self.note_record(&rec);
                let mut resp = self.instance_view(instance_id, Some(request_id), Some(false))?;
                if let Value::Obj(o) = &mut resp {
                    o.insert("applied".into(), Value::Bool(true));
                    o.insert("ok".into(), Value::Str("true".into()));
                    insert_configuration_fields(o, &response_tree, &a.configuration_after);
                    let mut tr = BTreeMap::new();
                    tr.insert("source_state".into(), Value::Str(a.source_state.clone()));
                    tr.insert(
                        "transition_idx".into(),
                        Value::Num(a.transition_idx.to_string()),
                    );
                    tr.insert("internal".into(), Value::Bool(a.internal));
                    if let Some(region) = &a.region {
                        tr.insert("region".into(), Value::Str(region.clone()));
                    }
                    insert_transition_configuration_fields(
                        &mut tr,
                        &from_configuration,
                        &a.configuration_after,
                    );
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
                self.finish_commit();
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
                body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
                body.insert("code".into(), Value::Str(r.code.into()));
                body.insert("message".into(), Value::Str(r.message.clone()));
                body.insert("hint".into(), Value::Str(r.hint.clone()));
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
                err = err.request_id(request_id);
                body.insert("details".into(), err.details.clone());
                if let Value::Obj(d) = &mut err.details {
                    d.insert("machine_id".into(), Value::Str(mid.clone()));
                    d.insert("instance_id".into(), Value::Str(instance_id.into()));
                }
                if let Some((s, e)) = err.span {
                    let mut sp = BTreeMap::new();
                    sp.insert("start".into(), Value::Num(s.to_string()));
                    sp.insert("end".into(), Value::Num(e.to_string()));
                    body.insert("span".into(), Value::Obj(sp));
                }
                let rec = self.append_at_with_root(
                    RecordKind::EventRejected,
                    Value::Obj(body),
                    commit_ts,
                )?;
                self.history
                    .entry(instance_id.into())
                    .or_default()
                    .push(rec.seq);
                self.note_record(&rec);
                let slot = self.claimed_slot(rec.seq);
                self.state.dedup.insert(request_id.into(), slot);
                self.last_errors.insert(request_id.into(), err.clone());
                self.finish_commit();
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
                body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
                let rec = self.append_at_with_root(
                    RecordKind::EventIgnored,
                    Value::Obj(body),
                    commit_ts,
                )?;
                self.note_record(&rec);
                let mut resp = self.instance_view(instance_id, Some(request_id), Some(false))?;
                if let Value::Obj(o) = &mut resp {
                    o.insert("ok".into(), Value::Str("true".into()));
                    o.insert("ignored".into(), Value::Bool(true));
                    o.insert("applied".into(), Value::Bool(false));
                    o.insert("seq".into(), Value::Num(rec.seq.to_string()));
                    o.insert("monitor_flags".into(), Value::Arr(vec![]));
                    o.insert("trace".into(), Value::Obj(BTreeMap::new()));
                    o.insert(
                        "transition".into(),
                        Value::Obj({
                            let mut transition = BTreeMap::from([
                                ("transition_idx".into(), Value::Num("-1".into())),
                                ("internal".into(), Value::Bool(false)),
                                ("exited".into(), Value::Arr(vec![])),
                                ("entered".into(), Value::Arr(vec![])),
                            ]);
                            if let Some(leaf) = inst.configuration.sequential_leaf() {
                                transition
                                    .insert("source_state".into(), Value::Str(leaf.to_string()));
                            }
                            insert_transition_configuration_fields(
                                &mut transition,
                                &inst.configuration,
                                &inst.configuration,
                            );
                            transition
                        }),
                    );
                }
                self.commit_dedup(request_id, resp.clone(), rec.seq);
                self.finish_commit();
                Ok(resp)
            }
        }
    }

    /// Poll and apply at most one due deadline using the process clock.
    pub fn poll_instance_deadline(
        &mut self,
        instance_id: &str,
        request_id: &str,
        expect_seq: Option<u64>,
    ) -> Result<Value, ErrorObj> {
        self.poll_instance_deadline_on(
            &mut crate::clock::GlobalClock,
            instance_id,
            request_id,
            expect_seq,
        )
    }

    /// Injected-clock form of [`Store::poll_instance_deadline`].
    pub fn poll_instance_deadline_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        instance_id: &str,
        request_id: &str,
        expect_seq: Option<u64>,
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        if let Some(result) = self.claim_request(request_id, Self::fp_poll_deadline(instance_id))? {
            return result;
        }
        if let Some(expected) = expect_seq {
            if expected != self.journal.last_seq {
                return Err(ErrorObj::new(
                    "req/seq_mismatch",
                    "re-read the instance, then retry with the same request_id and the current seq",
                )
                .hint(
                    "re-read the instance, then retry with the same request_id and the current seq",
                )
                .details(Value::Obj(BTreeMap::from([(
                    "current_seq".into(),
                    Value::Num(self.journal.last_seq.to_string()),
                )])))
                .request_id(request_id));
            }
        }
        let machine_id = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .ok_or_else(|| {
                ErrorObj::new("req/instance_not_found", instance_id)
                    .request_id(request_id)
                    .with_store_catalog(self)
            })?;
        let machine = self.state.machines.get(&machine_id).ok_or_else(|| {
            ErrorObj::new("req/machine_not_found", &machine_id).request_id(request_id)
        })?;
        let instance = self.state.instances.get(instance_id).ok_or_else(|| {
            ErrorObj::new("req/instance_not_found", instance_id).request_id(request_id)
        })?;
        let before_configuration = instance.configuration.clone();
        let commit_ts = clock.now_ms();
        let mut budget = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
        let outcome = poll_deadline(
            &machine.compiled,
            &machine.tree,
            instance,
            commit_ts,
            &mut budget,
        );
        match outcome {
            DeadlineOutcome::Applied(applied) => {
                let transition = applied.transition;
                let mut pending = instance.pending.clone();
                pending.extend(transition.effects.iter().map(|effect| {
                    format!("{instance_id}/{}/{}", self.journal.last_seq + 1, effect.k)
                }));
                let next_state = InstanceState {
                    status: transition.status_after,
                    configuration: transition.configuration_after.clone(),
                    ctx: transition.ctx_after.clone(),
                    history: transition.history_after.clone(),
                    deadlines: transition.deadlines_after.clone(),
                    pending,
                };
                let state_hash = state_hash(
                    &machine_id,
                    instance_id,
                    self.journal.last_seq + 1,
                    &next_state,
                );
                let body = Value::Obj(BTreeMap::from([
                    ("instance_id".into(), Value::Str(instance_id.into())),
                    ("request_id".into(), Value::Str(request_id.into())),
                    ("deadline".into(), Value::Str(applied.deadline.name.clone())),
                    (
                        "deadline_idx".into(),
                        Value::Num(applied.deadline.deadline_idx.to_string()),
                    ),
                    (
                        "due_ms".into(),
                        Value::Num(applied.deadline.due_ms.to_string()),
                    ),
                    ("state_hash".into(), Value::Str(state_hash)),
                    ("state_format".into(), Value::Str(STATE_FORMAT.into())),
                    (
                        "source_state".into(),
                        Value::Str(transition.source_state.clone()),
                    ),
                    (
                        "exited".into(),
                        Value::Arr(transition.exited.iter().cloned().map(Value::Str).collect()),
                    ),
                    (
                        "entered".into(),
                        Value::Arr(transition.entered.iter().cloned().map(Value::Str).collect()),
                    ),
                ]));
                let record =
                    self.append_at_with_root(RecordKind::DeadlineApplied, body, commit_ts)?;
                self.state.instances.insert(instance_id.into(), next_state);
                self.history
                    .entry(instance_id.into())
                    .or_default()
                    .push(record.seq);
                self.note_record(&record);
                let mut response =
                    self.instance_view(instance_id, Some(request_id), Some(false))?;
                if let Value::Obj(output) = &mut response {
                    output.insert("deadline_applied".into(), Value::Bool(true));
                    output.insert("deadline_not_due".into(), Value::Bool(false));
                    output.insert("deadline".into(), Value::Str(applied.deadline.name));
                    output.insert(
                        "deadline_idx".into(),
                        Value::Num(applied.deadline.deadline_idx.to_string()),
                    );
                    output.insert(
                        "due_ms".into(),
                        Value::Str(applied.deadline.due_ms.to_string()),
                    );
                    let mut transition_value = BTreeMap::from([
                        (
                            "source_state".into(),
                            Value::Str(transition.source_state.clone()),
                        ),
                        (
                            "deadline_idx".into(),
                            Value::Num(transition.transition_idx.to_string()),
                        ),
                        ("internal".into(), Value::Bool(false)),
                        (
                            "exited".into(),
                            Value::Arr(transition.exited.iter().cloned().map(Value::Str).collect()),
                        ),
                        (
                            "entered".into(),
                            Value::Arr(
                                transition.entered.iter().cloned().map(Value::Str).collect(),
                            ),
                        ),
                    ]);
                    if let Some(region) = &transition.region {
                        transition_value.insert("region".into(), Value::Str(region.clone()));
                    }
                    insert_transition_configuration_fields(
                        &mut transition_value,
                        &before_configuration,
                        &transition.configuration_after,
                    );
                    output.insert("transition".into(), Value::Obj(transition_value));
                    output.insert("trace".into(), transition.trace.to_value());
                    output.insert(
                        "monitor_flags".into(),
                        Value::Arr(
                            transition
                                .monitor_flags
                                .iter()
                                .cloned()
                                .map(Value::Str)
                                .collect(),
                        ),
                    );
                }
                self.commit_dedup(request_id, response.clone(), record.seq);
                self.finish_commit();
                Ok(response)
            }
            DeadlineOutcome::NotDue { next } => {
                let unchanged = instance.clone();
                let state_hash = state_hash(
                    &machine_id,
                    instance_id,
                    self.journal.last_seq + 1,
                    &unchanged,
                );
                let mut body = BTreeMap::from([
                    ("instance_id".into(), Value::Str(instance_id.into())),
                    ("request_id".into(), Value::Str(request_id.into())),
                    ("state_hash".into(), Value::Str(state_hash)),
                    ("state_format".into(), Value::Str(STATE_FORMAT.into())),
                ]);
                if let Some(next) = &next {
                    body.insert("next_deadline".into(), Value::Str(next.name.clone()));
                    body.insert(
                        "next_deadline_idx".into(),
                        Value::Num(next.deadline_idx.to_string()),
                    );
                    body.insert("next_due_ms".into(), Value::Num(next.due_ms.to_string()));
                }
                let record = self.append_at_with_root(
                    RecordKind::DeadlineNotDue,
                    Value::Obj(body),
                    commit_ts,
                )?;
                self.note_record(&record);
                self.history
                    .entry(instance_id.into())
                    .or_default()
                    .push(record.seq);
                let mut response =
                    self.instance_view(instance_id, Some(request_id), Some(false))?;
                if let Value::Obj(output) = &mut response {
                    output.insert("deadline_applied".into(), Value::Bool(false));
                    output.insert("deadline_not_due".into(), Value::Bool(true));
                    if let Some(next) = next {
                        output.insert("next_deadline".into(), Value::Str(next.name));
                        output.insert(
                            "next_deadline_idx".into(),
                            Value::Num(next.deadline_idx.to_string()),
                        );
                        output.insert("next_due_ms".into(), Value::Str(next.due_ms.to_string()));
                    }
                }
                self.commit_dedup(request_id, response.clone(), record.seq);
                self.finish_commit();
                Ok(response)
            }
            DeadlineOutcome::Rejected(rejected) => {
                let rejection = rejected.rejection;
                let unchanged = instance.clone();
                let state_hash = state_hash(
                    &machine_id,
                    instance_id,
                    self.journal.last_seq + 1,
                    &unchanged,
                );
                let mut error = ErrorObj::from_rejection(&rejection).request_id(request_id);
                let mut body = BTreeMap::from([
                    ("instance_id".into(), Value::Str(instance_id.into())),
                    ("request_id".into(), Value::Str(request_id.into())),
                    ("state_hash".into(), Value::Str(state_hash)),
                    ("state_format".into(), Value::Str(STATE_FORMAT.into())),
                    ("code".into(), Value::Str(rejection.code.into())),
                    ("message".into(), Value::Str(rejection.message.clone())),
                    ("hint".into(), Value::Str(rejection.hint.clone())),
                    ("details".into(), error.details.clone()),
                ]);
                let kind = if let Some(deadline) = rejected.deadline {
                    body.insert("deadline".into(), Value::Str(deadline.name));
                    body.insert(
                        "deadline_idx".into(),
                        Value::Num(deadline.deadline_idx.to_string()),
                    );
                    body.insert("due_ms".into(), Value::Num(deadline.due_ms.to_string()));
                    RecordKind::DeadlineRejected
                } else {
                    body.insert("operation".into(), Value::Str("poll_deadline".into()));
                    RecordKind::RequestRejected
                };
                if let Some((start, end)) = rejection.span {
                    body.insert(
                        "span".into(),
                        Value::Obj(BTreeMap::from([
                            ("start".into(), Value::Num(start.to_string())),
                            ("end".into(), Value::Num(end.to_string())),
                        ])),
                    );
                }
                if let Value::Obj(details) = &mut error.details {
                    details.insert("machine_id".into(), Value::Str(machine_id));
                    details.insert("instance_id".into(), Value::Str(instance_id.into()));
                }
                let record = self.append_at_with_root(kind, Value::Obj(body), commit_ts)?;
                self.note_record(&record);
                self.history
                    .entry(instance_id.into())
                    .or_default()
                    .push(record.seq);
                let slot = self.claimed_slot(record.seq);
                self.state.dedup.insert(request_id.into(), slot);
                self.last_errors.insert(request_id.into(), error.clone());
                self.finish_commit();
                Err(error)
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
        self.ack_effect_outcome_on(
            &mut crate::clock::GlobalClock,
            instance_id,
            effect_id,
            request_id,
            outcome,
            result,
        )
    }

    pub fn ack_effect_outcome_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        instance_id: &str,
        effect_id: &str,
        request_id: &str,
        outcome: &str,
        result: Option<Value>,
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        if let Some(r) = self.claim_request(
            request_id,
            Self::fp_ack(instance_id, effect_id, outcome, result.as_ref()),
        )? {
            return r;
        }
        if let Some(v) = result.as_ref() {
            Self::check_journalled_size("result", v, request_id)?;
        }
        if outcome != "ok" && outcome != "failed" {
            return Err(
                ErrorObj::new("req/args_invalid", "outcome must be ok or failed")
                    .request_id(request_id),
            );
        }
        let inst = self.state.instances.get(instance_id).ok_or_else(|| {
            ErrorObj::new("req/instance_not_found", instance_id).request_id(request_id)
        })?;
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
            det.insert("request_id".into(), Value::Str(request_id.into()));
            body.insert("details".into(), Value::Obj(det));
            body.insert("operation".into(), Value::Str("ack".into()));
            body.insert("effect_id".into(), Value::Str(effect_id.into()));
            let mid = self
                .state
                .instance_machines
                .get(instance_id)
                .cloned()
                .unwrap_or_default();
            let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, inst);
            body.insert("state_hash".into(), Value::Str(sh));
            body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
            let rec = self.append_rec(RecordKind::RequestRejected, Value::Obj(body), clock)?;
            self.note_record(&rec);
            let mut details = BTreeMap::new();
            details.insert(
                "pending".into(),
                Value::Arr(listed.into_iter().map(Value::Str).collect()),
            );
            let err = ErrorObj::new("req/field_unknown", "unknown effect id")
                .hint("use an id from effects_pending")
                .details(Value::Obj(details))
                .request_id(request_id);
            self.last_errors.insert(request_id.into(), err.clone());
            let slot = self.claimed_slot(rec.seq);
            self.state.dedup.insert(request_id.into(), slot);
            self.finish_commit();
            return Err(err);
        }
        let pending: Vec<String> = inst
            .pending
            .iter()
            .filter(|p| *p != effect_id)
            .cloned()
            .collect();
        let mid = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .ok_or_else(|| {
                ErrorObj::new("req/instance_not_found", instance_id).request_id(request_id)
            })?;
        let mut post = inst.clone();
        post.pending.clone_from(&pending);
        let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, &post);
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("effect_id".into(), Value::Str(effect_id.into()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("outcome".into(), Value::Str(outcome.into()));
        body.insert("state_hash".into(), Value::Str(sh));
        body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        if let Some(res) = result.clone() {
            body.insert("result".into(), res);
        }
        let rec = self.append_rec(RecordKind::EffectAcked, Value::Obj(body), clock)?;
        self.state.instances.insert(instance_id.into(), post);
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
        m.insert("request_id".into(), Value::Str(request_id.into()));
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
        self.finish_commit();
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
        self.cancel_instance_reason_on(
            &mut crate::clock::GlobalClock,
            instance_id,
            request_id,
            reason,
        )
    }

    pub fn cancel_instance_reason_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        instance_id: &str,
        request_id: &str,
        reason: &str,
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        if let Some(r) = self.claim_request(request_id, Self::fp_cancel(instance_id, reason))? {
            return r;
        }
        if !self.state.instances.contains_key(instance_id) {
            return Err(ErrorObj::new("req/instance_not_found", instance_id).request_id(request_id));
        }
        let mid = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .ok_or_else(|| {
                ErrorObj::new("req/instance_not_found", instance_id).request_id(request_id)
            })?;
        let mut post = self.state.instances.get(instance_id).unwrap().clone();
        post.status = Status::Cancelled;
        post.deadlines.clear();
        let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, &post);
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("reason".into(), Value::Str(reason.into()));
        body.insert("state_hash".into(), Value::Str(sh));
        body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        let rec = self.append_rec(RecordKind::InstanceCancelled, Value::Obj(body), clock)?;
        self.state.instances.insert(instance_id.into(), post);
        self.note_record(&rec);
        self.history
            .entry(instance_id.into())
            .or_default()
            .push(rec.seq);
        let resp = self.instance_view(instance_id, Some(request_id), Some(false))?;
        self.commit_dedup(request_id, resp.clone(), rec.seq);
        self.finish_commit();
        Ok(resp)
    }

    pub fn annotate(
        &mut self,
        instance_id: &str,
        request_id: &str,
        note: &str,
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        if let Some(r) = self.claim_request(request_id, Self::fp_annotate(instance_id, note))? {
            return r;
        }
        Self::check_journalled_size("note", &Value::Str(note.into()), request_id)?;
        if !self.state.instances.contains_key(instance_id) {
            return Err(ErrorObj::new("req/instance_not_found", instance_id).request_id(request_id));
        }
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("note".into(), Value::Str(note.into()));
        let rec = self.append_rec(
            RecordKind::Annotated,
            Value::Obj(body),
            &mut crate::clock::GlobalClock,
        )?;
        self.note_record(&rec);
        self.history
            .entry(instance_id.into())
            .or_default()
            .push(rec.seq);
        let mut m = BTreeMap::new();
        m.insert("ok".into(), Value::Str("true".into()));
        m.insert("note".into(), Value::Str(note.into()));
        m.insert("request_id".into(), Value::Str(request_id.into()));
        let resp = Value::Obj(m);
        self.commit_dedup(request_id, resp.clone(), rec.seq);
        self.finish_commit();
        Ok(resp)
    }

    pub fn instance_view(
        &self,
        instance_id: &str,
        request_id: Option<&str>,
        duplicate: Option<bool>,
    ) -> Result<Value, ErrorObj> {
        let inst = self.state.instances.get(instance_id).ok_or_else(|| {
            let e = ErrorObj::new("req/instance_not_found", instance_id)
                .hint("use a known instance id from details.known_instances")
                .with_store_catalog(self);
            match request_id {
                Some(rid) => e.request_id(rid),
                None => e,
            }
        })?;
        let mid = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .unwrap_or_default();
        let stored = self.state.machines.get(&mid);
        let mut ctx = BTreeMap::new();
        for (k, v) in &inst.ctx {
            ctx.insert(k.clone(), ctx_val_json(v));
        }
        let mut m = BTreeMap::new();
        m.insert("instance_id".into(), Value::Str(instance_id.into()));
        m.insert("ok".into(), Value::Str("true".into()));
        m.insert("status".into(), Value::Str(inst.status.as_str().into()));
        if let Some(st) = stored {
            insert_configuration_fields(&mut m, &st.tree, &inst.configuration);
            m.insert(
                "deadlines_pending".into(),
                pending_deadlines_value(st, inst),
            );
            let mut bud = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
            let evs = enabled_events(&st.compiled, &st.tree, inst, &mut bud);
            m.insert("enabled_events".into(), enabled_json(&evs));
            let mut mac = BTreeMap::new();
            mac.insert("machine_id".into(), Value::Str(mid.clone()));
            mac.insert("name".into(), Value::Str(st.compiled.spec.name.clone()));
            m.insert("machine".into(), Value::Obj(mac));
        } else {
            m.insert(
                "configuration".into(),
                configuration_value(&inst.configuration),
            );
            m.insert("deadlines_pending".into(), Value::Arr(vec![]));
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
        m.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        if let Some(r) = request_id {
            m.insert("request_id".into(), Value::Str(r.into()));
        }
        if let Some(d) = duplicate {
            m.insert("duplicate".into(), Value::Bool(d));
        }
        Ok(Value::Obj(m))
    }

    pub fn maybe_snapshot(&self) -> Result<(), ErrorObj> {
        self.ensure_writable()?;
        if self.journal.last_seq > 0 && self.journal.last_seq.is_multiple_of(10_000) {
            crate::snapshot::write_snapshot(&self.data_dir, &self.state)?;
        }
        Ok(())
    }

    fn checkpoint_for_snapshot_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
    ) -> Result<(), ErrorObj> {
        let current_root = crate::snapshot::materialize_state_root(&self.state);
        if self
            .records
            .last()
            .filter(|rec| rec.kind == RecordKind::StateCheckpoint)
            .and_then(|rec| rec.body.get("state_root").and_then(Value::as_str))
            == Some(current_root.as_str())
        {
            return Ok(());
        }
        let seq = self.journal.last_seq.saturating_add(1);
        let root = crate::snapshot::materialize_state_root_at(&self.state, seq);
        let rec = self
            .journal
            .append_at(
                RecordKind::StateCheckpoint,
                Value::Obj(BTreeMap::from([
                    ("state_root".into(), Value::Str(root)),
                    (
                        "state_root_format".into(),
                        Value::Str(STATE_ROOT_FORMAT.into()),
                    ),
                ])),
                clock.now_ms(),
            )
            .map_err(|error| Self::journal_write_error(error, None))?;
        self.note_record(&rec);
        Ok(())
    }

    pub fn shutdown_snapshot_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
    ) -> Result<(), ErrorObj> {
        self.ensure_writable()?;
        if self.journal.is_memory() || self.journal.last_seq == 0 {
            return Ok(());
        }
        self.checkpoint_for_snapshot_on(clock)?;
        crate::snapshot::write_snapshot(&self.data_dir, &self.state)?;
        Ok(())
    }

    pub fn shutdown_snapshot(&mut self) -> Result<(), ErrorObj> {
        self.shutdown_snapshot_on(&mut crate::clock::GlobalClock)
    }

    fn after_commit(&mut self) {
        let _ = self.maybe_snapshot();
    }

    pub fn history_page(
        &self,
        instance_id: &str,
        from_seq: u64,
        limit: usize,
        include_trace: bool,
        include_rejected: bool,
    ) -> Result<Value, ErrorObj> {
        let limit = limit.min(500);
        let mut entries = Vec::new();
        let mut next_from_seq = None;
        for rec in self.records.iter().filter(|r| {
            r.body.get("instance_id").and_then(Value::as_str) == Some(instance_id)
                && r.seq >= from_seq
        }) {
            if !include_rejected
                && matches!(
                    rec.kind,
                    RecordKind::EventRejected
                        | RecordKind::DeadlineRejected
                        | RecordKind::RequestRejected
                )
            {
                continue;
            }
            if entries.len() >= limit {
                next_from_seq = Some(rec.seq);
                break;
            }
            entries.push(history_entry(self, rec, include_trace)?);
        }
        let mut out = BTreeMap::from([
            ("instance_id".into(), Value::Str(instance_id.into())),
            ("entries".into(), Value::Arr(entries)),
            (
                "chain_verified".into(),
                Value::Bool(verify_prefix_hashes(&self.records)),
            ),
        ]);
        if let Some(n) = next_from_seq {
            out.insert("next_from_seq".into(), Value::Num(n.to_string()));
        }
        let _ = include_trace;
        Ok(Value::Obj(out))
    }

    pub fn explain_seq(&self, instance_id: &str, seq: u64) -> Result<Value, ErrorObj> {
        let rec = self
            .records
            .iter()
            .find(|r| r.seq == seq)
            .ok_or_else(|| ErrorObj::new("req/field_missing", "seq"))?;
        if rec.body.get("instance_id").and_then(Value::as_str) != Some(instance_id)
            && rec.kind != RecordKind::Genesis
            && rec.kind != RecordKind::MachineDefined
        {
            return Err(ErrorObj::new("req/instance_not_found", instance_id));
        }
        let mut e = history_entry(self, rec, true)?;
        if let Value::Obj(o) = &mut e {
            o.insert(
                "chain_verified".into(),
                Value::Bool(verify_prefix_hashes(&self.records)),
            );
        }
        Ok(e)
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        if !self.journal.is_memory() && !self.journal.is_read_only() && self.journal.last_seq > 0 {
            // Drop must never append: there is no caller-supplied clock and a
            // read-only open/close must leave the authoritative journal alone.
            let _ = crate::snapshot::write_snapshot(&self.data_dir, &self.state);
        }
    }
}

fn insert_configuration_fields(
    output: &mut BTreeMap<String, Value>,
    tree: &Tree,
    configuration: &ActiveConfiguration,
) {
    output.insert("configuration".into(), configuration_value(configuration));
    match configuration {
        ActiveConfiguration::Sequential { leaf } => {
            output.insert("leaf".into(), Value::Str(leaf.clone()));
            output.insert("state".into(), Value::Str(tree.dotted_path(leaf)));
            output.insert(
                "state_path".into(),
                Value::Arr(
                    tree.configuration(leaf)
                        .into_iter()
                        .map(Value::Str)
                        .collect(),
                ),
            );
        }
        ActiveConfiguration::Parallel { leaves } => {
            let mut regions = BTreeMap::new();
            for (region, _) in &tree.root_initials {
                let Some(region) = region.as_ref() else {
                    continue;
                };
                let Some(leaf) = leaves.get(region) else {
                    continue;
                };
                regions.insert(
                    region.clone(),
                    Value::Obj(BTreeMap::from([
                        ("leaf".into(), Value::Str(leaf.clone())),
                        ("state".into(), Value::Str(tree.dotted_path(leaf))),
                        (
                            "state_path".into(),
                            Value::Arr(
                                tree.configuration(leaf)
                                    .into_iter()
                                    .map(Value::Str)
                                    .collect(),
                            ),
                        ),
                    ])),
                );
            }
            output.insert("regions".into(), Value::Obj(regions));
        }
    }
}

fn insert_transition_configuration_fields(
    output: &mut BTreeMap<String, Value>,
    before: &ActiveConfiguration,
    after: &ActiveConfiguration,
) {
    output.insert("from_configuration".into(), configuration_value(before));
    output.insert("to_configuration".into(), configuration_value(after));
    if let (
        ActiveConfiguration::Sequential { leaf: from },
        ActiveConfiguration::Sequential { leaf: to },
    ) = (before, after)
    {
        output.insert("from_leaf".into(), Value::Str(from.clone()));
        output.insert("to_leaf".into(), Value::Str(to.clone()));
    }
}

fn pending_deadlines_value(machine: &StoredMachine, state: &InstanceState) -> Value {
    let mut pending: Vec<_> = state
        .deadlines
        .iter()
        .filter_map(|(name, due_ms)| {
            machine
                .compiled
                .spec
                .deadlines
                .iter()
                .position(|deadline| deadline.name == *name)
                .map(|index| (due_ms, index, name))
        })
        .collect();
    pending.sort_by(|left, right| (left.0, left.1).cmp(&(right.0, right.1)));
    Value::Arr(
        pending
            .into_iter()
            .map(|(due_ms, index, name)| {
                Value::Obj(BTreeMap::from([
                    ("name".into(), Value::Str(name.clone())),
                    ("deadline_idx".into(), Value::Num(index.to_string())),
                    ("due_ms".into(), Value::Str(due_ms.to_string())),
                ]))
            })
            .collect(),
    )
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
    let mut bud = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
    match step(&m.compiled, &m.tree, inst, ev, &payload, rec.ts, &mut bud) {
        Outcome::Applied(a) => {
            let mut post_inst = inst.clone();
            post_inst.status = a.status_after;
            post_inst.configuration = a.configuration_after.clone();
            post_inst.ctx = a.ctx_after.clone();
            post_inst.history = a.history_after.clone();
            post_inst.deadlines = a.deadlines_after.clone();
            post_inst.pending.extend(
                a.effects
                    .iter()
                    .map(|e| format!("{iid}/{}/{}", rec.seq, e.k)),
            );
            let mut post = pre.clone();
            post.instances.insert(iid.into(), post_inst);
            post.last_seq = rec.seq;
            post.last_hash = rec.hash.clone();
            let mut v = view_at(&post, iid, Some(request_id), Some(true), rec.seq).ok()?;
            if let Value::Obj(o) = &mut v {
                o.insert("applied".into(), Value::Bool(true));
                o.insert("ok".into(), Value::Str("true".into()));
                insert_configuration_fields(o, &m.tree, &a.configuration_after);
                let mut tr = BTreeMap::new();
                tr.insert("source_state".into(), Value::Str(a.source_state.clone()));
                tr.insert(
                    "transition_idx".into(),
                    Value::Num(a.transition_idx.to_string()),
                );
                tr.insert("internal".into(), Value::Bool(a.internal));
                if let Some(region) = &a.region {
                    tr.insert("region".into(), Value::Str(region.clone()));
                }
                insert_transition_configuration_fields(
                    &mut tr,
                    &inst.configuration,
                    &a.configuration_after,
                );
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

fn reconstruct_deadline_applied(
    pre: &StoreState,
    record: &Record,
    instance_id: &str,
    request_id: &str,
) -> Option<Value> {
    let machine_id = pre.instance_machines.get(instance_id)?;
    let machine = pre.machines.get(machine_id)?;
    let instance = pre.instances.get(instance_id)?;
    let mut budget = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
    let DeadlineOutcome::Applied(applied) = poll_deadline(
        &machine.compiled,
        &machine.tree,
        instance,
        record.ts,
        &mut budget,
    ) else {
        return None;
    };
    let transition = applied.transition;
    let mut post_instance = instance.clone();
    post_instance.status = transition.status_after;
    post_instance.configuration = transition.configuration_after.clone();
    post_instance.ctx = transition.ctx_after.clone();
    post_instance.history = transition.history_after.clone();
    post_instance.deadlines = transition.deadlines_after.clone();
    post_instance.pending.extend(
        transition
            .effects
            .iter()
            .map(|effect| format!("{instance_id}/{}/{}", record.seq, effect.k)),
    );
    let mut post = pre.clone();
    post.instances.insert(instance_id.into(), post_instance);
    post.last_seq = record.seq;
    post.last_hash = record.hash.clone();
    let mut response =
        view_at(&post, instance_id, Some(request_id), Some(true), record.seq).ok()?;
    if let Value::Obj(output) = &mut response {
        output.insert("deadline_applied".into(), Value::Bool(true));
        output.insert("deadline_not_due".into(), Value::Bool(false));
        output.insert("deadline".into(), Value::Str(applied.deadline.name));
        output.insert(
            "deadline_idx".into(),
            Value::Num(applied.deadline.deadline_idx.to_string()),
        );
        output.insert(
            "due_ms".into(),
            Value::Str(applied.deadline.due_ms.to_string()),
        );
        let mut transition_value = BTreeMap::from([
            (
                "source_state".into(),
                Value::Str(transition.source_state.clone()),
            ),
            (
                "deadline_idx".into(),
                Value::Num(transition.transition_idx.to_string()),
            ),
            ("internal".into(), Value::Bool(false)),
            (
                "exited".into(),
                Value::Arr(transition.exited.iter().cloned().map(Value::Str).collect()),
            ),
            (
                "entered".into(),
                Value::Arr(transition.entered.iter().cloned().map(Value::Str).collect()),
            ),
        ]);
        if let Some(region) = &transition.region {
            transition_value.insert("region".into(), Value::Str(region.clone()));
        }
        insert_transition_configuration_fields(
            &mut transition_value,
            &instance.configuration,
            &transition.configuration_after,
        );
        output.insert("transition".into(), Value::Obj(transition_value));
        output.insert("trace".into(), transition.trace.to_value());
        output.insert(
            "monitor_flags".into(),
            Value::Arr(
                transition
                    .monitor_flags
                    .iter()
                    .cloned()
                    .map(Value::Str)
                    .collect(),
            ),
        );
    }
    Some(response)
}

fn reconstruct_ignored(
    folded: &StoreState,
    rec: &Record,
    iid: &str,
    request_id: &str,
) -> Option<Value> {
    let inst = folded.instances.get(iid)?;
    let mid = folded.instance_machines.get(iid)?;
    let m = folded.machines.get(mid)?;
    let mut v = view_at(folded, iid, Some(request_id), Some(true), rec.seq).ok()?;
    if let Value::Obj(o) = &mut v {
        o.insert("ok".into(), Value::Str("true".into()));
        o.insert("ignored".into(), Value::Bool(true));
        o.insert("applied".into(), Value::Bool(false));
        o.insert("seq".into(), Value::Num(rec.seq.to_string()));
        o.insert("monitor_flags".into(), Value::Arr(vec![]));
        o.insert("trace".into(), Value::Obj(BTreeMap::new()));
        o.insert(
            "transition".into(),
            Value::Obj({
                let mut transition = BTreeMap::from([
                    ("transition_idx".into(), Value::Num("-1".into())),
                    ("internal".into(), Value::Bool(false)),
                    ("exited".into(), Value::Arr(vec![])),
                    ("entered".into(), Value::Arr(vec![])),
                ]);
                if let Some(leaf) = inst.configuration.sequential_leaf() {
                    transition.insert("source_state".into(), Value::Str(leaf.to_string()));
                }
                insert_transition_configuration_fields(
                    &mut transition,
                    &inst.configuration,
                    &inst.configuration,
                );
                transition
            }),
        );
        insert_configuration_fields(o, &m.tree, &inst.configuration);
    }
    Some(v)
}

fn load_tags_from_records(records: &[Record]) -> BTreeMap<String, Vec<String>> {
    let mut tags = BTreeMap::new();
    for rec in records {
        if rec.kind != RecordKind::InstanceCreated {
            continue;
        }
        let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) else {
            continue;
        };
        if let Some(arr) = rec.body.get("tags").and_then(Value::as_arr) {
            let v: Vec<String> = arr
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            if !v.is_empty() {
                tags.insert(iid.into(), v);
            }
        }
    }
    tags
}

fn verify_prefix_hashes(records: &[Record]) -> bool {
    records.windows(2).all(|w| w[1].prev == w[0].hash)
}

fn history_entry(store: &Store, rec: &Record, include_trace: bool) -> Result<Value, ErrorObj> {
    let mut e = BTreeMap::new();
    e.insert("seq".into(), Value::Num(rec.seq.to_string()));
    e.insert("kind".into(), Value::Str(format!("{:?}", rec.kind)));
    e.insert("ts".into(), Value::Num(rec.ts.to_string()));
    e.insert("hash".into(), Value::Str(rec.hash.clone()));
    if let Some(rid) = rec.body.get("request_id") {
        e.insert("request_id".into(), rid.clone());
    }
    if let Some(ev) = rec.body.get("event") {
        e.insert("event".into(), ev.clone());
    }
    if let Some(deadline) = rec.body.get("deadline") {
        e.insert("deadline".into(), deadline.clone());
    }
    if let Some(p) = rec.body.get("payload") {
        e.insert("payload".into(), p.clone());
    }
    if let Some(n) = rec.body.get("note") {
        e.insert("note".into(), n.clone());
    }
    if let Some(r) = rec.body.get("reason") {
        e.insert("reason".into(), r.clone());
    }
    if rec.seq > 0 {
        if let Ok(pre) = fold_prefix(&store.records, rec.seq.saturating_sub(1)) {
            if let Ok(post) = fold_prefix(&store.records, rec.seq) {
                if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                    if let Some(before) = pre.instances.get(iid) {
                        e.insert(
                            "from_configuration".into(),
                            configuration_value(&before.configuration),
                        );
                        e.insert(
                            "before_configuration".into(),
                            configuration_value(&before.configuration),
                        );
                        if let Some(leaf) = before.configuration.sequential_leaf() {
                            e.insert("from_leaf".into(), Value::Str(leaf.to_string()));
                            e.insert("before_leaf".into(), Value::Str(leaf.to_string()));
                        }
                        let mut ctx = BTreeMap::new();
                        for (k, v) in &before.ctx {
                            ctx.insert(k.clone(), ctx_val_json(v));
                        }
                        e.insert("before_context".into(), Value::Obj(ctx));
                    }
                    if let Some(after) = post.instances.get(iid) {
                        e.insert(
                            "to_configuration".into(),
                            configuration_value(&after.configuration),
                        );
                        e.insert(
                            "after_configuration".into(),
                            configuration_value(&after.configuration),
                        );
                        if let Some(leaf) = after.configuration.sequential_leaf() {
                            e.insert("to_leaf".into(), Value::Str(leaf.to_string()));
                            e.insert("after_leaf".into(), Value::Str(leaf.to_string()));
                        }
                        let mut ctx = BTreeMap::new();
                        for (k, v) in &after.ctx {
                            ctx.insert(k.clone(), ctx_val_json(v));
                        }
                        e.insert("context_after".into(), Value::Obj(ctx.clone()));
                        e.insert("after_context".into(), Value::Obj(ctx));
                        if !e.contains_key("from_configuration") {
                            e.insert(
                                "from_configuration".into(),
                                configuration_value(&after.configuration),
                            );
                            if let Some(leaf) = after.configuration.sequential_leaf() {
                                e.insert("from_leaf".into(), Value::Str(leaf.to_string()));
                            }
                        }
                    }
                }
            }
            if include_trace && rec.kind == RecordKind::EventApplied {
                if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                    if let Some(rid) = rec.body.get("request_id").and_then(Value::as_str) {
                        if let Some(v) = reconstruct_applied(&pre, rec, iid, rid) {
                            if let Some(tr) = v.get("trace") {
                                e.insert("trace".into(), tr.clone());
                            }
                        }
                    }
                }
            } else if include_trace && rec.kind == RecordKind::DeadlineApplied {
                if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                    if let Some(rid) = rec.body.get("request_id").and_then(Value::as_str) {
                        if let Some(value) = reconstruct_deadline_applied(&pre, rec, iid, rid) {
                            if let Some(trace) = value.get("trace") {
                                e.insert("trace".into(), trace.clone());
                            }
                        }
                    }
                }
            } else if include_trace && rec.kind == RecordKind::EventRejected {
                if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                    if let Some(ev) = rec.body.get("event").and_then(Value::as_str) {
                        if let Some(mid) = pre.instance_machines.get(iid) {
                            if let Some(m) = pre.machines.get(mid) {
                                if let Some(inst) = pre.instances.get(iid) {
                                    let payload = rec
                                        .body
                                        .get("payload")
                                        .cloned()
                                        .unwrap_or(Value::Obj(BTreeMap::new()));
                                    let mut bud = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
                                    if let Outcome::Rejected(r) = step(
                                        &m.compiled,
                                        &m.tree,
                                        inst,
                                        ev,
                                        &payload,
                                        rec.ts,
                                        &mut bud,
                                    ) {
                                        e.insert("trace".into(), r.trace.to_value());
                                    }
                                }
                            }
                        }
                    }
                }
            } else if include_trace && rec.kind == RecordKind::DeadlineRejected {
                if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                    if let Some(machine_id) = pre.instance_machines.get(iid) {
                        if let (Some(machine), Some(instance)) =
                            (pre.machines.get(machine_id), pre.instances.get(iid))
                        {
                            let mut budget = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
                            if let DeadlineOutcome::Rejected(rejected) = poll_deadline(
                                &machine.compiled,
                                &machine.tree,
                                instance,
                                rec.ts,
                                &mut budget,
                            ) {
                                e.insert("trace".into(), rejected.rejection.trace.to_value());
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(Value::Obj(e))
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
    let mut bud = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
    let enabled = enabled_events(&m.compiled, &m.tree, inst, &mut bud);
    let mut ctx = BTreeMap::new();
    for (k, v) in &inst.ctx {
        ctx.insert(k.clone(), ctx_val_json(v));
    }
    let mut mobj = BTreeMap::new();
    mobj.insert("ok".into(), Value::Str("true".into()));
    mobj.insert("instance_id".into(), Value::Str(instance_id.into()));
    insert_configuration_fields(&mut mobj, &m.tree, &inst.configuration);
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
    mobj.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
    mobj.insert("enabled_events".into(), enabled_json(&enabled));
    mobj.insert("deadlines_pending".into(), pending_deadlines_value(m, inst));
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
        JournalHealth::VersionMismatch { .. } => "store/version_mismatch",
        JournalHealth::StoreIo(_) => "io/read",
        JournalHealth::Ok => "store/lock",
    };
    let err = ErrorObj::new(code, h.message());
    if matches!(h, JournalHealth::VersionMismatch { .. }) {
        // Post-migration this fires for newer or unknown formats, where the
        // store may be the only good copy — never advise deleting it.
        err.hint("upgrade fsm to a build that supports this store format, or point --data-dir at a fresh directory")
    } else if matches!(h, JournalHealth::StoreIo(_)) {
        err.hint("restore the named persistence path as a readable regular file or directory within the documented per-unit limit, then retry")
    } else {
        err
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

pub fn context_not_object(got: &str) -> ErrorObj {
    let mut details = BTreeMap::new();
    details.insert("field".into(), Value::Str("context".into()));
    details.insert("expected".into(), Value::Str("object".into()));
    details.insert("got".into(), Value::Str(got.into()));
    ErrorObj::new("req/args_invalid", "expected object")
        .hint("set context to object")
        .details(Value::Obj(details))
}

pub fn number_token_error(field: &str) -> ErrorObj {
    ErrorObj::new("req/number_token", field).hint(format!("send {field} as a JSON string"))
}

pub fn apply_context_overrides(
    spec: &MachineSpec,
    ctx: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Val>, ErrorObj> {
    let mut overrides = BTreeMap::new();
    for (k, val) in ctx {
        let raw = match val {
            Value::Str(s) => s.clone(),
            Value::Num(_) => return Err(number_token_error(k)),
            Value::Bool(b) => b.to_string(),
            _ => return Err(ErrorObj::new("req/field_type", k.clone())),
        };
        let decl = spec
            .context
            .iter()
            .find(|c| c.name == *k)
            .ok_or_else(|| ErrorObj::new("req/field_unknown", k.clone()))?;
        overrides.insert(k.clone(), coerce_ctx_override(&decl.ty, k, &raw)?);
    }
    Ok(overrides)
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
        // Shares the reader in core so a hand-supplied override and a
        // journalled one parse identically; accepts `premium` and `tier.premium`.
        TySpec::Enum { .. } => fsm_core::replay::parse_ctx_val(ty, raw)
            .ok_or_else(|| ErrorObj::new("req/field_type", key)),
    }
}

#[allow(dead_code)]
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

    /// Per-process counter. Tests in one binary run concurrently and a
    /// timestamp alone collides: two threads landing in the same nanosecond
    /// bucket share a directory, and one wipes the other's store mid-run. It
    /// showed up first on a fast macOS release build.
    static TMP_N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn tmp() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pid = std::process::id();
        let i = TMP_N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("fsm-s-{pid}-{n}-{i}"));
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

    fn timed_parallel_def() -> Value {
        parse(
            br#"{
                "format":"fsm.machine/1","name":"timed_parallel",
                "context":[{"name":"fires","ty":"int","init":"0"}],
                "events":[{"name":"finish","fields":[]}],
                "regions":[
                    {"name":"timer","states":[{"name":"waiting"},{"name":"expired","terminal":true}],"initial":"waiting"},
                    {"name":"work","states":[{"name":"working"},{"name":"done","terminal":true}],"initial":"working"}
                ],
                "transitions":[{"from":"working","on":"finish","to":"done"}],
                "deadlines":[{"name":"expire","from":"waiting","after":"dur(10, ms)","to":"expired","do":[{"target":"fires","value":"ctx.fires + 1"}]}]
            }"#,
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
        assert_eq!(r1.get("configuration"), r2.get("configuration"));
        assert_eq!(r1.get("context"), r2.get("context"));
        assert_eq!(r1.get("effects_pending"), r2.get("effects_pending"));
        assert_eq!(r1.get("enabled_events"), r2.get("enabled_events"));
        assert_eq!(r1.get("state_hash"), r2.get("state_hash"));
    }

    #[test]
    fn deadline_poll_is_durable_idempotent_and_parallel_safe() {
        let dir = tmp();
        let mut store = Store::open(&dir).unwrap();
        let mut define_clock = crate::clock::FixedClock::new(1, 1);
        store
            .define_machine_on(&mut define_clock, timed_parallel_def(), false, false)
            .unwrap();
        let mut create_clock = crate::clock::FixedClock::new(100, 1);
        store
            .create_instance_ctx_on(
                &mut create_clock,
                "timed_parallel",
                "timed-1",
                "create-1",
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
        assert_eq!(create_clock.now, 101, "creation reads its clock once");
        assert_eq!(
            store
                .state
                .instances
                .get("timed-1")
                .unwrap()
                .deadlines
                .get("expire"),
            Some(&110)
        );

        let mut snapshot_clock = crate::clock::FixedClock::new(101, 1);
        store.shutdown_snapshot_on(&mut snapshot_clock).unwrap();
        drop(store);
        let mut store = Store::open(&dir).unwrap();
        assert_eq!(
            store
                .state
                .instances
                .get("timed-1")
                .unwrap()
                .deadlines
                .get("expire"),
            Some(&110)
        );

        let mut early_clock = crate::clock::FixedClock::new(109, 1);
        let early = store
            .poll_instance_deadline_on(&mut early_clock, "timed-1", "poll-early", None)
            .unwrap();
        assert_eq!(early_clock.now, 110, "poll reads its clock once");
        assert_eq!(
            early.get("deadline_not_due").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            early.get("next_deadline").and_then(Value::as_str),
            Some("expire")
        );
        assert_eq!(
            early.get("next_due_ms").and_then(Value::as_str),
            Some("110")
        );
        let early_seq = store.journal.last_seq;

        let mut retry_clock = crate::clock::FixedClock::new(999, 1);
        let retry = store
            .poll_instance_deadline_on(&mut retry_clock, "timed-1", "poll-early", Some(0))
            .unwrap();
        assert_eq!(retry_clock.now, 999, "dedup precedes the clock read");
        assert_eq!(store.journal.last_seq, early_seq);
        assert_eq!(retry.get("duplicate").and_then(Value::as_bool), Some(true));
        assert_eq!(retry.get("next_deadline"), early.get("next_deadline"));
        assert_eq!(
            retry.get("next_deadline_idx"),
            early.get("next_deadline_idx")
        );
        assert_eq!(retry.get("next_due_ms"), early.get("next_due_ms"));

        let mut due_clock = crate::clock::FixedClock::new(110, 1);
        let fired = store
            .poll_instance_deadline_on(&mut due_clock, "timed-1", "poll-due", None)
            .unwrap();
        assert_eq!(due_clock.now, 111);
        assert_eq!(
            fired.get("deadline_applied").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            fired.get("deadline").and_then(Value::as_str),
            Some("expire")
        );
        let configuration = fired.get("configuration").and_then(Value::as_obj).unwrap();
        assert_eq!(
            configuration.get("kind").and_then(Value::as_str),
            Some("parallel")
        );
        assert_eq!(
            fired
                .get("context")
                .and_then(Value::as_obj)
                .and_then(|context| context.get("fires"))
                .and_then(Value::as_str),
            Some("1")
        );
        assert_eq!(
            store.records.last().map(|record| record.kind),
            Some(RecordKind::DeadlineApplied)
        );

        let fired_seq = store.journal.last_seq;
        drop(store);
        let mut reopened = Store::open(&dir).unwrap();
        let mut lost_response_clock = crate::clock::FixedClock::new(5_000, 1);
        let duplicate = reopened
            .poll_instance_deadline_on(&mut lost_response_clock, "timed-1", "poll-due", None)
            .unwrap();
        assert_eq!(lost_response_clock.now, 5_000);
        assert_eq!(reopened.journal.last_seq, fired_seq);
        assert_eq!(
            duplicate.get("deadline_applied").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            duplicate.get("duplicate").and_then(Value::as_bool),
            Some(true)
        );

        let mut finish_payload = Value::Obj(BTreeMap::new());
        let mut finish_clock = crate::clock::FixedClock::new(111, 1);
        let completed = reopened
            .send_event_stamp_on(
                &mut finish_clock,
                "timed-1",
                "finish",
                &mut finish_payload,
                "finish-1",
                None,
                &[],
            )
            .unwrap();
        assert_eq!(
            completed.get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert!(
            reopened
                .state
                .instances
                .get("timed-1")
                .unwrap()
                .deadlines
                .is_empty()
        );
    }

    #[test]
    fn cancelled_deadline_poll_rejection_is_durable() {
        let dir = tmp();
        let mut store = Store::open(&dir).unwrap();
        store
            .define_machine(timed_parallel_def(), false, false)
            .unwrap();
        store
            .create_instance("timed_parallel", "timed-2", "create-2", None)
            .unwrap();
        store
            .cancel_instance_reason("timed-2", "cancel-2", "operator")
            .unwrap();
        assert!(
            store
                .state
                .instances
                .get("timed-2")
                .unwrap()
                .deadlines
                .is_empty()
        );
        let mut clock = crate::clock::FixedClock::new(1_000, 1);
        let error = store
            .poll_instance_deadline_on(&mut clock, "timed-2", "poll-cancelled", None)
            .unwrap_err();
        assert_eq!(error.code, "run/instance_cancelled");
        assert_eq!(
            store.records.last().map(|record| record.kind),
            Some(RecordKind::RequestRejected)
        );
        drop(store);
        let mut reopened = Store::open(&dir).unwrap();
        let mut retry_clock = crate::clock::FixedClock::new(2_000, 1);
        let duplicate = reopened
            .poll_instance_deadline_on(&mut retry_clock, "timed-2", "poll-cancelled", None)
            .unwrap_err();
        assert_eq!(duplicate, error.mark_duplicate());
        assert_eq!(retry_clock.now, 2_000);
    }

    fn strip_dup(v: &Value) -> Value {
        let mut c = v.clone();
        if let Value::Obj(o) = &mut c {
            o.remove("duplicate");
        }
        c
    }

    #[test]
    fn reopen_retry_matches_original_bytes() {
        let dir = tmp();
        let mut s = Store::open(&dir).unwrap();
        s.define_machine(case_def(), false, false).unwrap();
        s.create_instance("case_review", "i1", "c1", None).unwrap();
        let r1 = s
            .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
            .unwrap();
        let _ = s
            .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "S", None)
            .unwrap();
        drop(s);
        let mut s2 = Store::open(&dir).unwrap();
        let r2 = s2
            .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
            .unwrap();
        assert_eq!(r2.get("duplicate").and_then(Value::as_bool), Some(true));
        assert_eq!(strip_dup(&r1), strip_dup(&r2));
        assert_eq!(
            r2.get("state_path")
                .and_then(Value::as_arr)
                .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>()),
            Some(vec!["in_review", "docs_review"])
        );
    }

    #[test]
    fn allocator_skips_explicit_ids() {
        let dir = tmp();
        let mut s = Store::open(&dir).unwrap();
        s.define_machine(case_def(), false, false).unwrap();
        let before = s.journal.last_seq;
        let taken = format!("req-{}-{}", before + 1, before + 2);
        s.create_instance("case_review", "i1", &taken, None)
            .unwrap();
        assert_eq!(s.journal.last_seq, before + 1);
        let next = s.allocate_request_id().unwrap();
        assert_ne!(next, taken);
        assert_eq!(next, format!("req-{}-{}", before + 1, before + 3));
        fs::write(dir.join("alloc"), "").unwrap();
        drop(s);
        let mut s2 = Store::open(&dir).unwrap();
        let after_torn = s2.allocate_request_id().unwrap();
        assert!(!s2.state.dedup.contains_key(&after_torn));
        assert_ne!(after_torn, taken);
        assert!(after_torn.starts_with("req-"));
    }

    #[test]
    fn ack_and_annotate_retry_keep_shape() {
        let dir = tmp();
        let mut s = Store::open(&dir).unwrap();
        s.define_machine(case_def(), false, false).unwrap();
        s.create_instance("case_review", "i1", "c1", None).unwrap();
        s.send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
            .unwrap();
        let eid = s
            .state
            .instances
            .get("i1")
            .unwrap()
            .pending
            .first()
            .cloned()
            .unwrap();
        let a1 = s
            .ack_effect_outcome("i1", &eid, "ack1", "ok", None)
            .unwrap();
        let n1 = s.annotate("i1", "n1", "hello").unwrap();
        drop(s);
        let mut s2 = Store::open(&dir).unwrap();
        let a2 = s2
            .ack_effect_outcome("i1", &eid, "ack1", "ok", None)
            .unwrap();
        let n2 = s2.annotate("i1", "n1", "hello").unwrap();
        assert_eq!(
            a2.get("effect_id").and_then(Value::as_str),
            Some(eid.as_str())
        );
        assert_eq!(a2.get("acked").and_then(Value::as_bool), Some(true));
        assert_eq!(strip_dup(&a1), strip_dup(&a2));
        assert_eq!(n2.get("note").and_then(Value::as_str), Some("hello"));
        assert_eq!(strip_dup(&n1), strip_dup(&n2));
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

    #[test]
    fn rejected_retry_keeps_request_id() {
        let dir = tmp();
        let mut s = Store::open(&dir).unwrap();
        s.define_machine(case_def(), false, false).unwrap();
        s.create_instance("case_review", "i1", "c1", None).unwrap();
        s.send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
            .unwrap();
        let e1 = s
            .send_event("i1", "resume", Value::Obj(BTreeMap::new()), "r", None)
            .unwrap_err();
        assert_eq!(
            e1.details.get("request_id").and_then(Value::as_str),
            Some("r")
        );
        let pending = s.state.instances.get("i1").unwrap().pending.clone();
        let ae1 = s
            .ack_effect_outcome("i1", "nope", "ar", "ok", None)
            .unwrap_err();
        assert_eq!(
            ae1.details.get("request_id").and_then(Value::as_str),
            Some("ar")
        );
        assert!(!pending.is_empty());
        drop(s);
        let mut s2 = Store::open(&dir).unwrap();
        let e2 = s2
            .send_event("i1", "resume", Value::Obj(BTreeMap::new()), "r", None)
            .unwrap_err();
        assert!(!e1.duplicate);
        assert!(e2.duplicate);
        let mut e1b = e1.clone();
        let mut e2b = e2.clone();
        e1b.duplicate = false;
        e2b.duplicate = false;
        assert_eq!(e1b.to_value(), e2b.to_value());
        let ae2 = s2
            .ack_effect_outcome("i1", "nope", "ar", "ok", None)
            .unwrap_err();
        assert!(!ae1.duplicate);
        assert!(ae2.duplicate);
        let mut a1b = ae1.clone();
        let mut a2b = ae2.clone();
        a1b.duplicate = false;
        a2b.duplicate = false;
        assert_eq!(a1b.to_value(), a2b.to_value());
    }
}
