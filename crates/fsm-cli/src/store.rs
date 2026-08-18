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
use fsm_core::spec::{Finding, MachineSpec, TySpec};
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
    pub tags: BTreeMap<String, Vec<String>>,
    pub replayed_records: usize,
    pub opened_from_snapshot: bool,
    pub opened_snapshot_seq: Option<u64>,
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
            if let (Some(s), Some(idx)) = (r.source_state.as_ref(), r.transition_idx) {
                d.insert("source_state".into(), Value::Str(s.clone()));
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
        let mut sink = HistSink {
            history: BTreeMap::new(),
            records: Vec::new(),
        };
        let (journal, state, open_path) = match journal_io::open(data_dir, &mut sink) {
            Ok(x) => x,
            Err(OpenError::Health(h)) => return Err(health_err(&h)),
            Err(OpenError::Io(s)) => return Err(ErrorObj::new("io/read", s)),
        };
        fs::create_dir_all(data_dir.join("snapshots")).ok();
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
            if rec.kind == RecordKind::EventRejected {
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

    fn note_record(&mut self, rec: &Record) {
        self.records.push(rec.clone());
        self.state.last_seq = rec.seq;
        self.state.last_hash = rec.hash.clone();
    }

    fn finish_commit(&mut self) {
        self.after_commit();
    }

    fn append_at_with_root(
        &mut self,
        kind: RecordKind,
        body: Value,
        ts: i64,
    ) -> Result<Record, ErrorObj> {
        let seq = self.journal.last_seq.saturating_add(1);
        if seq % 10_000 != 0 {
            return self
                .journal
                .append_at(kind, body, ts)
                .map_err(|e| ErrorObj::new("io/write", e.to_string()));
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
        self.journal
            .append_at(kind, Value::Obj(body), ts)
            .map_err(|e| ErrorObj::new("io/write", e.to_string()))
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
        let path = self.data_dir.join("alloc");
        let n = fs::read_to_string(&path)
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
                fs::write(&tmp, format!("{next}\n"))
                    .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
                let f =
                    fs::File::open(&tmp).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
                f.sync_all()
                    .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
                fs::rename(&tmp, &path).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
                let dirf = fs::File::open(&self.data_dir)
                    .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
                dirf.sync_all()
                    .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
                return Ok(cand);
            }
            next += 1;
        }
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
        if let Some(r) = self.replay_request(request_id) {
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
        let a = create(&m.compiled, &m.tree, overrides).map_err(|r| {
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
        if !tags.is_empty() {
            body.insert(
                "tags".into(),
                Value::Arr(tags.iter().cloned().map(Value::Str).collect()),
            );
        }
        let rec = self.append_rec(RecordKind::InstanceCreated, Value::Obj(body), clock)?;
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
        if let Some(r) = self.replay_request(request_id) {
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
        let inst = self.state.instances.get(instance_id).ok_or_else(|| {
            ErrorObj::new("req/instance_not_found", instance_id)
                .request_id(request_id)
                .hint("use a known instance id from details.known_instances")
                .with_store_catalog(self)
        })?;
        let from_leaf = inst.leaf.clone();
        let mut absent_stamps: Vec<String> = Vec::new();
        if let Value::Obj(o) = payload {
            for field in stamps {
                if !o.contains_key(*field) {
                    absent_stamps.push((*field).into());
                    o.insert((*field).into(), Value::Str("0".into()));
                }
            }
        }
        if let Err(r) = validate_event(&m.compiled, event, payload) {
            return Err(ErrorObj::from_rejection(&r).request_id(request_id));
        }
        let commit_ts = clock.now_ms();
        if let Value::Obj(o) = payload {
            if !absent_stamps.is_empty() {
                let ts = commit_ts.to_string();
                for field in &absent_stamps {
                    o.insert(field.clone(), Value::Str(ts.clone()));
                }
            }
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
                    o.insert("leaf".into(), Value::Str(a.leaf_after.clone()));
                    let mut tr = BTreeMap::new();
                    tr.insert("source_state".into(), Value::Str(a.source_state.clone()));
                    tr.insert(
                        "transition_idx".into(),
                        Value::Num(a.transition_idx.to_string()),
                    );
                    tr.insert("internal".into(), Value::Bool(a.internal));
                    tr.insert("from_leaf".into(), Value::Str(from_leaf.clone()));
                    tr.insert("to_leaf".into(), Value::Str(a.leaf_after.clone()));
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
                self.state.dedup.insert(request_id.into(), rec.seq);
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
                        Value::Obj(BTreeMap::from([
                            ("source_state".into(), Value::Str(inst.leaf.clone())),
                            ("transition_idx".into(), Value::Num("-1".into())),
                            ("internal".into(), Value::Bool(false)),
                            ("from_leaf".into(), Value::Str(inst.leaf.clone())),
                            ("to_leaf".into(), Value::Str(inst.leaf.clone())),
                            ("exited".into(), Value::Arr(vec![])),
                            ("entered".into(), Value::Arr(vec![])),
                        ])),
                    );
                }
                self.commit_dedup(request_id, resp.clone(), rec.seq);
                self.finish_commit();
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
        if let Some(r) = self.replay_request(request_id) {
            return r;
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
            self.state.dedup.insert(request_id.into(), rec.seq);
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
        if let Some(r) = self.replay_request(request_id) {
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
        let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, &post);
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("reason".into(), Value::Str(reason.into()));
        body.insert("state_hash".into(), Value::Str(sh));
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
        if let Some(r) = self.replay_request(request_id) {
            return r;
        }
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
                Value::Obj(BTreeMap::from([("state_root".into(), Value::Str(root))])),
                clock.now_ms(),
            )
            .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        self.note_record(&rec);
        Ok(())
    }

    pub fn shutdown_snapshot_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
    ) -> Result<(), ErrorObj> {
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
                    RecordKind::EventRejected | RecordKind::RequestRejected
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
        if !self.journal.is_memory() && self.journal.last_seq > 0 {
            // Drop must never append: there is no caller-supplied clock and a
            // read-only open/close must leave the authoritative journal alone.
            let _ = crate::snapshot::write_snapshot(&self.data_dir, &self.state);
        }
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
            let mut post_inst = inst.clone();
            post_inst.status = a.status_after;
            post_inst.leaf = a.leaf_after.clone();
            post_inst.ctx = a.ctx_after.clone();
            post_inst.history = a.history_after.clone();
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
                tr.insert("from_leaf".into(), Value::Str(inst.leaf.clone()));
                tr.insert("to_leaf".into(), Value::Str(a.leaf_after.clone()));
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
            Value::Obj(BTreeMap::from([
                ("source_state".into(), Value::Str(inst.leaf.clone())),
                ("transition_idx".into(), Value::Num("-1".into())),
                ("internal".into(), Value::Bool(false)),
                ("from_leaf".into(), Value::Str(inst.leaf.clone())),
                ("to_leaf".into(), Value::Str(inst.leaf.clone())),
                ("exited".into(), Value::Arr(vec![])),
                ("entered".into(), Value::Arr(vec![])),
            ])),
        );
        o.insert("state".into(), Value::Str(m.tree.dotted_path(&inst.leaf)));
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
                        e.insert("from_leaf".into(), Value::Str(before.leaf.clone()));
                        e.insert("before_leaf".into(), Value::Str(before.leaf.clone()));
                        let mut ctx = BTreeMap::new();
                        for (k, v) in &before.ctx {
                            ctx.insert(k.clone(), val_json(v));
                        }
                        e.insert("before_context".into(), Value::Obj(ctx));
                    }
                    if let Some(after) = post.instances.get(iid) {
                        e.insert("to_leaf".into(), Value::Str(after.leaf.clone()));
                        e.insert("after_leaf".into(), Value::Str(after.leaf.clone()));
                        let mut ctx = BTreeMap::new();
                        for (k, v) in &after.ctx {
                            ctx.insert(k.clone(), val_json(v));
                        }
                        e.insert("context_after".into(), Value::Obj(ctx.clone()));
                        e.insert("after_context".into(), Value::Obj(ctx));
                        if !e.contains_key("from_leaf") {
                            e.insert("from_leaf".into(), Value::Str(after.leaf.clone()));
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
                                    let mut bud = Budget::new(4096);
                                    if let Outcome::Rejected(r) =
                                        step(&m.compiled, &m.tree, inst, ev, &payload, &mut bud)
                                    {
                                        e.insert("trace".into(), r.trace.to_value());
                                    }
                                }
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
        JournalHealth::VersionMismatch { .. } => "store/version_mismatch",
        JournalHealth::StoreIo(_) => "io/read",
        JournalHealth::Ok => "store/lock",
    };
    let err = ErrorObj::new(code, h.message());
    if matches!(h, JournalHealth::VersionMismatch { .. }) {
        // Post-migration this fires for newer or unknown formats, where the
        // store may be the only good copy — never advise deleting it.
        err.hint("upgrade fsm to a build that supports this store format, or point --data-dir at a fresh directory")
    } else {
        err
    }
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
        TySpec::Enum { of } => Ok(Val::Enum {
            ty: of.clone(),
            variant: raw.into(),
        }),
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
        assert_eq!(r1.get("configuration"), r2.get("configuration"));
        assert_eq!(r1.get("context"), r2.get("context"));
        assert_eq!(r1.get("effects_pending"), r2.get("effects_pending"));
        assert_eq!(r1.get("enabled_events"), r2.get("enabled_events"));
        assert_eq!(r1.get("state_hash"), r2.get("state_hash"));
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
            r2.get("configuration")
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
