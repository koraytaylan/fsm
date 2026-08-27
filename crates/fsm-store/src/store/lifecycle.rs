use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use fsm_core::hashes::{ResolveError, machine_id, resolve_machine_ref};
use fsm_core::json::Value;
use fsm_core::record::RecordKind;
use fsm_core::replay::{NopSink, StoredMachine, fold_with};
use fsm_core::spec::Finding;
use fsm_core::tree::Tree;

use crate::journal_io::{self, Journal, OpenError};

use super::reconstruct::{health_err, load_tags_from_records};
use super::{ErrorObj, HistSink, Store};

pub struct DefineOutcome {
    pub created: bool,
    pub machine_id: String,
    pub warnings: Vec<Finding>,
    pub name: String,
}

/// The derived indexes an open builds.
struct Indexes {
    history: BTreeMap<String, Vec<u64>>,
    parents: BTreeMap<String, (String, String)>,
    machine_seqs: BTreeMap<String, u64>,
}

impl Store {
    /// The store-side indexes, built from one complete record vector.
    ///
    /// Both are derived, never journaled: `history` is every record touching
    /// an instance, in seq order, so its first entry is the record that
    /// brought the instance into existence — an `instance_created` for a
    /// root, an `instance_invoked` for a child, which is the only uniform
    /// answer to "when did this appear" once children exist. `parents` is the
    /// invocation edge, which has to be remembered because a child id derives
    /// from its parent through a hash and a hash does not invert.
    /// `machine_seqs` is when each definition arrived, so "newest first" is a
    /// map lookup rather than a scan of the journal per machine — which is
    /// what a listing and an interactive completion both need it to be.
    fn index_records(records: &[fsm_core::record::Record]) -> Indexes {
        let mut history: BTreeMap<String, Vec<u64>> = BTreeMap::new();
        let mut parents: BTreeMap<String, (String, String)> = BTreeMap::new();
        let mut machine_seqs: BTreeMap<String, u64> = BTreeMap::new();
        for record in records {
            for instance_id in fsm_core::record::instances_touched(record) {
                history
                    .entry(instance_id.into())
                    .or_default()
                    .push(record.seq);
            }
            if record.kind == fsm_core::record::RecordKind::InstanceInvoked {
                let field = |name: &str| record.body.get(name).and_then(Value::as_str);
                if let (Some(parent), Some(slot), Some(child)) = (
                    field("parent_instance_id"),
                    field("slot"),
                    field("child_instance_id"),
                ) {
                    parents.insert(child.into(), (parent.into(), slot.into()));
                }
            }
            if record.kind == fsm_core::record::RecordKind::MachineDefined
                && let Some(machine_id) = record.body.get("machine_id").and_then(Value::as_str)
            {
                // First definition wins: redefining an identical spec is the
                // same machine, and its age is when it first appeared.
                machine_seqs.entry(machine_id.into()).or_insert(record.seq);
            }
        }
        Indexes {
            history,
            parents,
            machine_seqs,
        }
    }

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
        let Indexes {
            history,
            parents,
            machine_seqs,
        } = Self::index_records(&records);
        let tags = load_tags_from_records(&records);
        Ok(Store {
            journal,
            state,
            history,
            parents,
            machine_seqs,
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
        let Indexes {
            history,
            parents,
            machine_seqs,
        } = Self::index_records(&records);
        let tags = load_tags_from_records(&records);
        Ok(Store {
            journal,
            state,
            history,
            parents,
            machine_seqs,
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
            parents: BTreeMap::new(),
            machine_seqs: BTreeMap::new(),
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

    /// The machines this store holds, keyed by the 64-hex digest an `invoke`
    /// slot names.
    ///
    /// A superseded machine is **never** removed from this catalogue, and a
    /// future cleanup that wants to collect them starts here and must stop:
    /// every record an instance wrote before it migrated replays against the
    /// definition it was on, and a pending effect's name re-derives from the
    /// machine that emitted it. A store that garbage-collected superseded
    /// definitions would be a store that cannot fold its own journal.
    pub fn invoke_catalogue(&self) -> fsm_core::spec::Catalogue {
        self.state
            .machines
            .iter()
            .filter_map(|(machine_id, stored)| {
                fsm_core::hashes::digest_of(machine_id)
                    .map(|digest| (digest.to_string(), stored.compiled.spec.clone()))
            })
            .collect()
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
        // A definition that invokes is compiled against the machines this
        // store holds: the child's declarations type its done-event payload,
        // and the five catalogue rules `4801` deferred are decidable only
        // here, where the child definitions exist.
        let catalogue = self.invoke_catalogue();
        let compiled = fsm_core::spec::compile_accepted_with_catalogue(&def, &catalogue)
            .map_err(ErrorObj::from_findings)?;
        let catalogue_findings = fsm_core::spec::validate_catalogue(&compiled, &catalogue);
        if !catalogue_findings.is_empty() {
            return Err(ErrorObj::from_findings(catalogue_findings));
        }
        // A `supersedes` mapping is checked here for the same reason: the
        // superseded definition has to be in hand. A definition that names a
        // machine this store does not hold is refused rather than accepted
        // and failed later, because a mapping nobody can check is a mapping
        // nobody should trust.
        if let Some(supersedes) = &compiled.spec.supersedes {
            let Some(old_spec) = catalogue.get(&supersedes.machine) else {
                return Err(ErrorObj::from_findings(vec![fsm_core::spec::Finding::err(
                    "def/supersedes_unknown_machine",
                    "/supersedes/machine",
                    format!("this store holds no machine {}", supersedes.machine),
                    "define the superseded machine first; a mapping is checked against it",
                )]));
            };
            let old_compiled =
                fsm_core::spec::compile(old_spec.clone()).map_err(ErrorObj::from_findings)?;
            let old_tree = Tree::for_machine(&old_compiled.spec);
            let new_tree = Tree::for_machine(&compiled.spec);
            let findings = fsm_core::migrate::validate::validate_supersedes(
                &old_compiled,
                &old_tree,
                &compiled,
                &new_tree,
            );
            if !findings.is_empty() {
                return Err(ErrorObj::from_findings(findings));
            }
        }
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
}
