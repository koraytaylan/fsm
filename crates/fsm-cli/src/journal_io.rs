//! Append-only journal segments with per-record fsync.

#![allow(clippy::all, unused)]
//!
//! Advisory `journal/LOCK` is released by the OS on process death; the pid
//! metadata is diagnostic only and is never trusted for liveness. There is
//! deliberately no stale-lock heuristic.

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::{Record, RecordError, RecordKind, limits_value, seal, verify_line, zeros};
use fsm_core::replay::{NopSink, RecordSink, StoreState, fold_with};

const ROTATE_RECORDS: u32 = 65_536;
const ROTATE_BYTES: u64 = 64 * 1024 * 1024;

pub fn should_rotate(seg_bytes: u64, seg_records: u32) -> bool {
    seg_records >= ROTATE_RECORDS || seg_bytes >= ROTATE_BYTES
}

pub struct Journal {
    pub dir: PathBuf,
    seg: File,
    pub seg_name: String,
    pub seg_first_seq: u64,
    pub seg_bytes: u64,
    pub seg_records: u32,
    pub last_seq: u64,
    pub last_hash: String,
    pub poisoned: bool,
    _lock: File,
}

#[derive(Debug)]
pub enum JournalIoError {
    Locked { pid: i64 },
    Io(String),
    Poisoned,
    Record(RecordError),
    VersionMismatch { found: String },
}

impl std::fmt::Display for JournalIoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JournalIoError::Locked { pid } => {
                write!(f, "another process owns this store (pid {pid})")
            }
            JournalIoError::Io(s) => write!(f, "{s}"),
            JournalIoError::Poisoned => write!(f, "journal is poisoned"),
            JournalIoError::Record(_) => write!(f, "record error"),
            JournalIoError::VersionMismatch { found } => {
                write!(f, "store/version_mismatch: found {found}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalHealth {
    Ok,
    TornTail {
        segment: String,
        offset: u64,
        bytes: u64,
    },
    ChainBroken {
        seq: u64,
        segment: String,
        offset: u64,
        expected: String,
        found: String,
    },
    StateHashMismatch {
        seq: u64,
    },
    NonCanonical {
        seq: u64,
        segment: String,
        offset: u64,
    },
    LockIo(String),
    ReplayMismatch {
        seq: u64,
        field: String,
    },
    MissingGenesis,
    VersionMismatch {
        found: String,
    },
}

impl JournalHealth {
    pub fn message(&self) -> String {
        match self {
            JournalHealth::Ok => "ok".into(),
            JournalHealth::TornTail {
                segment,
                offset,
                bytes,
            } => {
                format!(
                    "torn tail in {segment} at {offset} ({bytes} bytes); fsm repair --truncate-torn-tail"
                )
            }
            JournalHealth::ChainBroken {
                seq,
                segment,
                offset,
                expected,
                found,
            } => {
                format!(
                    "chain broken at seq {seq} in {segment} offset {offset} expected {expected} found {found}; records ≥ {seq} unverifiable"
                )
            }
            JournalHealth::StateHashMismatch { seq } => format!("state hash mismatch at {seq}"),
            JournalHealth::NonCanonical {
                seq,
                segment,
                offset,
            } => {
                format!("non-canonical record seq {seq} in {segment} at {offset}")
            }
            JournalHealth::LockIo(s) => s.clone(),
            JournalHealth::ReplayMismatch { seq, field } => {
                format!("replay mismatch at seq {seq} field {field}")
            }
            JournalHealth::MissingGenesis => {
                "journal has no genesis record; delete the data directory and recreate".into()
            }
            JournalHealth::VersionMismatch { found } => {
                format!(
                    "store format {found} is not compatible; delete the data directory and recreate"
                )
            }
        }
    }
}

#[derive(Debug)]
pub enum OpenError {
    Health(JournalHealth),
    Io(String),
}

pub fn journal_dir(data: &Path) -> PathBuf {
    data.join("journal")
}

fn acquire_lock(jdir: &Path) -> Result<File, JournalIoError> {
    fs::create_dir_all(jdir).map_err(|e| JournalIoError::Io(e.to_string()))?;
    let path = jdir.join("LOCK");
    let mut f = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|e| JournalIoError::Io(e.to_string()))?;
    if f.try_lock().is_err() {
        let mut buf = String::new();
        let _ = f.read_to_string(&mut buf);
        let pid = parse(buf.as_bytes(), &JsonLimits::DEFAULT)
            .ok()
            .and_then(|v| {
                v.get("pid")
                    .and_then(Value::as_num)
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(0);
        return Err(JournalIoError::Locked { pid });
    }
    f.set_len(0)
        .map_err(|e| JournalIoError::Io(e.to_string()))?;
    let pid = std::process::id();
    let ts = crate::clock::now_ms();
    let line = format!("{{\"pid\":{pid},\"started_ts\":{ts}}}\n");
    f.write_all(line.as_bytes())
        .map_err(|e| JournalIoError::Io(e.to_string()))?;
    f.sync_all()
        .map_err(|e| JournalIoError::Io(e.to_string()))?;
    Ok(f)
}

fn sync_dir(dir: &Path) -> Result<(), JournalIoError> {
    let f = File::open(dir).map_err(|e| JournalIoError::Io(e.to_string()))?;
    f.sync_all()
        .map_err(|e| JournalIoError::Io(e.to_string()))?;
    Ok(())
}

fn seg_name(first: u64) -> String {
    format!("seg-{first:020}.jsonl")
}

fn write_genesis_unlocked(jdir: &Path) -> Result<(), JournalIoError> {
    fs::create_dir_all(jdir).map_err(|e| JournalIoError::Io(e.to_string()))?;
    if classify_has_genesis(jdir) {
        return Ok(());
    }
    let name = seg_name(0);
    let path = jdir.join(&name);
    let mut seg = OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(&path)
        .map_err(|e| JournalIoError::Io(e.to_string()))?;
    let mut body = std::collections::BTreeMap::new();
    body.insert("format".into(), Value::Str("fsm.journal/1".into()));
    body.insert(
        "created_ts".into(),
        Value::Num(crate::clock::now_ms().to_string()),
    );
    body.insert("limits".into(), limits_value());
    let rec = seal(
        0,
        crate::clock::now_ms(),
        RecordKind::Genesis,
        Value::Obj(body),
        &zeros(),
    );
    let line = rec.to_line();
    let existing = fs::read(&path).unwrap_or_default();
    if !existing.is_empty() {
        return Ok(());
    }
    seg.write_all(&line)
        .map_err(|e| JournalIoError::Io(e.to_string()))?;
    seg.sync_all()
        .map_err(|e| JournalIoError::Io(e.to_string()))?;
    sync_dir(jdir)?;
    Ok(())
}

fn classify_has_genesis(jdir: &Path) -> bool {
    let dir = jdir.parent().unwrap_or(jdir);
    matches!(classify(dir), JournalHealth::Ok)
}

pub(crate) fn write_store_version(dir: &Path) -> Result<(), String> {
    if let Err(h) = require_store_format(dir) {
        return Err(format!("store/version_mismatch: {}", h.message()));
    }
    write_version_durable(dir).map_err(|e| e.to_string())
}

fn from_health(h: JournalHealth) -> JournalIoError {
    match h {
        JournalHealth::VersionMismatch { found } => JournalIoError::VersionMismatch { found },
        other => JournalIoError::Io(other.message()),
    }
}

fn write_version_durable(dir: &Path) -> Result<(), JournalIoError> {
    fs::create_dir_all(dir).map_err(|e| JournalIoError::Io(e.to_string()))?;
    let ver = dir.join("VERSION");
    if ver.exists() {
        return Ok(());
    }
    let tmp = dir.join(format!(
        "VERSION.{}.{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::write(&tmp, "5\n").map_err(|e| JournalIoError::Io(e.to_string()))?;
    let f = File::open(&tmp).map_err(|e| JournalIoError::Io(e.to_string()))?;
    f.sync_all()
        .map_err(|e| JournalIoError::Io(e.to_string()))?;
    match fs::rename(&tmp, &ver) {
        Ok(()) => {}
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            if ver.exists() {
                return Ok(());
            }
            return Err(JournalIoError::Io(e.to_string()));
        }
    }
    sync_dir(dir)
}

pub fn init(dir: &Path) -> Result<Journal, JournalIoError> {
    if let Err(h) = require_store_format(dir) {
        return Err(from_health(h));
    }
    let jdir = journal_dir(dir);
    fs::create_dir_all(&jdir).map_err(|e| JournalIoError::Io(e.to_string()))?;
    let lock = acquire_lock(&jdir)?;
    if let Err(h) = require_store_format(dir) {
        drop(lock);
        return Err(from_health(h));
    }
    write_version_durable(dir)?;
    write_genesis_unlocked(&jdir)?;
    drop(lock);
    let mut sink = fsm_core::replay::NopSink;
    open(dir, &mut sink).map(|(j, _)| j).map_err(|e| match e {
        OpenError::Io(s) => JournalIoError::Io(s),
        OpenError::Health(h) => JournalIoError::Io(h.message()),
    })
}

impl Journal {
    pub fn append(&mut self, kind: RecordKind, body: Value) -> Result<Record, JournalIoError> {
        if self.poisoned {
            return Err(JournalIoError::Poisoned);
        }
        if should_rotate(self.seg_bytes, self.seg_records) {
            if let Err(e) = self.rotate() {
                self.poisoned = true;
                return Err(e);
            }
        }
        let rec = seal(
            self.last_seq + 1,
            crate::clock::now_ms(),
            kind,
            body,
            &self.last_hash,
        );
        let line = rec.to_line();
        if let Err(e) = self.seg.write_all(&line).and_then(|_| self.seg.sync_all()) {
            self.poisoned = true;
            return Err(JournalIoError::Io(e.to_string()));
        }
        self.seg_bytes += line.len() as u64;
        self.seg_records += 1;
        self.last_seq = rec.seq;
        self.last_hash = rec.hash.clone();
        Ok(rec)
    }

    fn rotate(&mut self) -> Result<(), JournalIoError> {
        let next = self.last_seq + 1;
        let name = seg_name(next);
        let path = journal_dir(&self.dir).join(&name);
        let seg = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)
            .map_err(|e| JournalIoError::Io(e.to_string()))?;
        sync_dir(&journal_dir(&self.dir))?;
        self.seg = seg;
        self.seg_name = name;
        self.seg_first_seq = next;
        self.seg_bytes = 0;
        self.seg_records = 0;
        Ok(())
    }
}

pub fn classify(dir: &Path) -> JournalHealth {
    let jdir = journal_dir(dir);
    let mut segs: Vec<_> = match fs::read_dir(&jdir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("seg-") && n.ends_with(".jsonl"))
                    .unwrap_or(false)
            })
            .collect(),
        Err(e) => return JournalHealth::LockIo(e.to_string()),
    };
    segs.sort();
    if segs.is_empty() {
        return JournalHealth::MissingGenesis;
    }
    let mut expect_seq = 0u64;
    let mut expect_prev = zeros();
    let mut saw_record = false;
    for (si, path) in segs.iter().enumerate() {
        let last_seg = si + 1 == segs.len();
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) => return JournalHealth::LockIo(e.to_string()),
        };
        let segn = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut off = 0usize;
        let mut start = 0usize;
        while start < bytes.len() {
            let end = bytes[start..]
                .iter()
                .position(|&b| b == b'\n')
                .map(|i| start + i + 1)
                .unwrap_or(bytes.len());
            let line = &bytes[start..end];
            let is_last_line = end == bytes.len();
            match verify_line(line, expect_seq, &expect_prev) {
                Ok(rec) => {
                    if rec.seq == 0 && rec.kind != RecordKind::Genesis {
                        return JournalHealth::ChainBroken {
                            seq: 0,
                            segment: segn,
                            offset: start as u64,
                            expected: "genesis".into(),
                            found: format!("{:?}", rec.kind),
                        };
                    }
                    expect_seq = rec.seq + 1;
                    expect_prev = rec.hash;
                    off = end;
                    start = end;
                    saw_record = true;
                }
                Err(_) if last_seg && is_last_line && !line.ends_with(&[b'\n']) => {
                    return JournalHealth::TornTail {
                        segment: segn,
                        offset: start as u64,
                        bytes: (bytes.len() - start) as u64,
                    };
                }
                Err(RecordError::NonCanonical { seq, .. }) => {
                    return JournalHealth::NonCanonical {
                        seq,
                        segment: segn,
                        offset: start as u64,
                    };
                }
                Err(RecordError::SeqGap { seq, expected }) => {
                    return JournalHealth::ChainBroken {
                        seq: expected,
                        segment: segn,
                        offset: start as u64,
                        expected: expected.to_string(),
                        found: seq.to_string(),
                    };
                }
                Err(RecordError::HashMismatch { seq }) => {
                    return JournalHealth::ChainBroken {
                        seq,
                        segment: segn,
                        offset: start as u64,
                        expected: expect_prev.clone(),
                        found: "hash".into(),
                    };
                }
                Err(RecordError::Parse { .. })
                | Err(RecordError::PrevMismatch { .. })
                | Err(RecordError::BodyInvalid { .. }) => {
                    return JournalHealth::ChainBroken {
                        seq: expect_seq,
                        segment: segn,
                        offset: start as u64,
                        expected: expect_prev,
                        found: "parse".into(),
                    };
                }
            }
        }
        let _ = off;
    }
    if !saw_record {
        return JournalHealth::MissingGenesis;
    }
    JournalHealth::Ok
}

fn replay_health(e: fsm_core::replay::ReplayError) -> JournalHealth {
    match e {
        fsm_core::replay::ReplayError::StateHashMismatch { seq, .. } => {
            JournalHealth::StateHashMismatch { seq }
        }
        fsm_core::replay::ReplayError::FieldMismatch { seq, field } => {
            JournalHealth::ReplayMismatch {
                seq,
                field: field.into(),
            }
        }
        fsm_core::replay::ReplayError::UnknownMachine { seq } => JournalHealth::ReplayMismatch {
            seq,
            field: "machine".into(),
        },
        fsm_core::replay::ReplayError::UnknownInstance { seq } => JournalHealth::ReplayMismatch {
            seq,
            field: "instance".into(),
        },
    }
}

fn active_segment_meta(jdir: &Path, recs: &[Record]) -> Result<(String, u64, u64, u32), String> {
    let mut segs: Vec<_> = fs::read_dir(jdir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("seg-") && n.ends_with(".jsonl"))
        .collect();
    segs.sort();
    let name = segs.pop().unwrap_or_else(|| seg_name(0));
    let path = jdir.join(&name);
    let bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let first = name
        .strip_prefix("seg-")
        .and_then(|s| s.strip_suffix(".jsonl"))
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let count = recs.iter().filter(|r| r.seq >= first).count() as u32;
    Ok((name, first, bytes, count))
}

pub fn open(dir: &Path, sink: &mut impl RecordSink) -> Result<(Journal, StoreState), OpenError> {
    if let Err(h) = require_store_format(dir) {
        return Err(OpenError::Health(h));
    }
    let jdir = journal_dir(dir);
    fs::create_dir_all(&jdir).map_err(|e| OpenError::Io(e.to_string()))?;
    let lock = acquire_lock(&jdir).map_err(|e| match e {
        JournalIoError::Locked { pid } => {
            OpenError::Health(JournalHealth::LockIo(format!("locked {pid}")))
        }
        other => OpenError::Io(other.to_string()),
    })?;
    if let Err(h) = require_store_format(dir) {
        return Err(OpenError::Health(h));
    }
    write_version_durable(dir).map_err(|e| OpenError::Io(e.to_string()))?;
    let health = classify(dir);
    if matches!(health, JournalHealth::MissingGenesis) {
        write_genesis_unlocked(&jdir).map_err(|e| OpenError::Io(e.to_string()))?;
    }
    let health = classify(dir);
    if !matches!(health, JournalHealth::Ok) {
        return Err(OpenError::Health(health));
    }
    let recs = load_records(dir).map_err(OpenError::Io)?;
    let state = crate::snapshot::open_state(dir, recs.clone(), sink)
        .map_err(|e| OpenError::Health(replay_health(e)))?;
    let last = recs.last();
    let (name, first, bytes, count) = active_segment_meta(&jdir, &recs).map_err(OpenError::Io)?;
    let path = jdir.join(&name);
    let seg = OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(&path)
        .map_err(|e| OpenError::Io(e.to_string()))?;
    Ok((
        Journal {
            dir: dir.to_path_buf(),
            seg,
            seg_name: name,
            seg_first_seq: first,
            seg_bytes: bytes,
            seg_records: count,
            last_seq: last.map(|r| r.seq).unwrap_or(0),
            last_hash: last.map(|r| r.hash.clone()).unwrap_or_else(zeros),
            poisoned: false,
            _lock: lock,
        },
        state,
    ))
}

pub fn load_records(dir: &Path) -> Result<Vec<Record>, String> {
    let jdir = journal_dir(dir);
    let mut segs: Vec<_> = fs::read_dir(&jdir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("seg-"))
                .unwrap_or(false)
        })
        .collect();
    segs.sort();
    let mut out = Vec::new();
    let mut expect = 0u64;
    let mut prev = zeros();
    for path in segs {
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        for line in bytes.split_inclusive(|&b| b == b'\n') {
            if line.iter().all(|b| b.is_ascii_whitespace()) {
                continue;
            }
            let rec = verify_line(line, expect, &prev).map_err(|e| format!("{e:?}"))?;
            expect = rec.seq + 1;
            prev = rec.hash.clone();
            out.push(rec);
        }
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct VerifyReport {
    pub health: JournalHealth,
    pub records: u64,
    pub machines: u64,
    pub instances: u64,
}

pub fn require_store_format(dir: &Path) -> Result<(), JournalHealth> {
    let ver = dir.join("VERSION");
    let jdir = journal_dir(dir);
    let has_journal = jdir.exists()
        && fs::read_dir(&jdir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .any(|e| e.file_name().to_string_lossy().starts_with("seg-"))
            })
            .unwrap_or(false);
    if ver.exists() {
        let v = fs::read_to_string(&ver).unwrap_or_default();
        let t = v.trim();
        if t == "1" || t == "2" || t == "3" || t == "4" {
            return Err(JournalHealth::VersionMismatch { found: t.into() });
        }
        if t != "5" {
            return Err(JournalHealth::VersionMismatch {
                found: t.to_string(),
            });
        }
        return Ok(());
    }
    if has_journal {
        return Err(JournalHealth::VersionMismatch { found: "1".into() });
    }
    Ok(())
}

pub fn verify(dir: &Path) -> VerifyReport {
    if let Err(h) = require_store_format(dir) {
        return VerifyReport {
            health: h,
            records: 0,
            machines: 0,
            instances: 0,
        };
    }
    let health = classify(dir);
    if !matches!(health, JournalHealth::Ok) {
        return VerifyReport {
            health,
            records: 0,
            machines: 0,
            instances: 0,
        };
    }
    let recs = match load_records(dir) {
        Ok(r) => r,
        Err(e) => {
            return VerifyReport {
                health: JournalHealth::LockIo(e),
                records: 0,
                machines: 0,
                instances: 0,
            };
        }
    };
    match fold_with(recs.clone(), &mut NopSink) {
        Ok(st) => VerifyReport {
            health: JournalHealth::Ok,
            records: recs.len() as u64,
            machines: st.machines.len() as u64,
            instances: st.instances.len() as u64,
        },
        Err(e) => VerifyReport {
            health: replay_health(e),
            records: recs.len() as u64,
            machines: 0,
            instances: 0,
        },
    }
}

#[derive(Debug)]
pub struct RepairReport {
    pub quarantined: PathBuf,
    pub bytes: u64,
    pub truncated_to_seq: u64,
}

#[derive(Debug)]
pub enum RepairError {
    NothingToRepair,
    Interior(JournalHealth),
    Io(String),
}

pub fn repair_truncate_torn_tail(dir: &Path) -> Result<RepairReport, RepairError> {
    if let Err(h) = require_store_format(dir) {
        return Err(RepairError::Interior(h));
    }
    let jdir = journal_dir(dir);
    let _lock = acquire_lock(&jdir).map_err(|e| RepairError::Io(e.to_string()))?;
    let health = classify(dir);
    match health {
        JournalHealth::Ok => Err(RepairError::NothingToRepair),
        JournalHealth::TornTail {
            segment,
            offset,
            bytes,
        } => {
            let path = jdir.join(&segment);
            let data = fs::read(&path).map_err(|e| RepairError::Io(e.to_string()))?;
            if data.len() < offset as usize {
                return Err(RepairError::Io("stale torn-tail offset".into()));
            }
            let tail = data[offset as usize..].to_vec();
            let recs = load_prefix(&data[..offset as usize]);
            let mut chain = load_prior_records(dir, &segment).map_err(RepairError::Io)?;
            chain.extend(recs.iter().cloned());
            if let Err(e) = fold_with(chain, &mut NopSink) {
                return Err(RepairError::Interior(replay_health(e)));
            }
            let qdir = jdir.join("quarantine");
            fs::create_dir_all(&qdir).map_err(|e| RepairError::Io(e.to_string()))?;
            sync_dir(&jdir).map_err(|e| RepairError::Io(e.to_string()))?;
            let seg_first = segment
                .strip_prefix("seg-")
                .and_then(|s| s.strip_suffix(".jsonl"))
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let first_bad = recs.last().map(|r| r.seq + 1).unwrap_or(seg_first);
            let mut qpath = qdir.join(format!("{segment}-tail-{first_bad}.bin"));
            let mut n = 0u32;
            let qf = loop {
                match OpenOptions::new().write(true).create_new(true).open(&qpath) {
                    Ok(f) => break f,
                    Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                        n += 1;
                        qpath = qdir.join(format!("{segment}-tail-{first_bad}-{n}.bin"));
                    }
                    Err(e) => return Err(RepairError::Io(e.to_string())),
                }
            };
            {
                let mut qf = qf;
                qf.write_all(&tail)
                    .map_err(|e| RepairError::Io(e.to_string()))?;
                qf.sync_all().map_err(|e| RepairError::Io(e.to_string()))?;
            }
            sync_dir(&qdir).map_err(|e| RepairError::Io(e.to_string()))?;
            sync_dir(&jdir).map_err(|e| RepairError::Io(e.to_string()))?;
            let mut f = OpenOptions::new()
                .write(true)
                .open(&path)
                .map_err(|e| RepairError::Io(e.to_string()))?;
            f.set_len(offset)
                .map_err(|e| RepairError::Io(e.to_string()))?;
            f.sync_all().map_err(|e| RepairError::Io(e.to_string()))?;
            sync_dir(&jdir).map_err(|e| RepairError::Io(e.to_string()))?;
            if offset == 0 && seg_first == 0 {
                write_genesis_unlocked(&jdir).map_err(|e| RepairError::Io(e.to_string()))?;
            }
            let after = classify(dir);
            if !matches!(after, JournalHealth::Ok) {
                return Err(RepairError::Interior(after));
            }
            let kept = load_records(dir).map_err(RepairError::Io)?;
            if let Err(e) = fold_with(kept, &mut NopSink) {
                return Err(RepairError::Interior(replay_health(e)));
            }
            Ok(RepairReport {
                quarantined: qpath,
                bytes,
                truncated_to_seq: recs
                    .last()
                    .map(|r| r.seq)
                    .unwrap_or(seg_first.saturating_sub(1)),
            })
        }
        other => Err(RepairError::Interior(other)),
    }
}

fn load_prior_records(dir: &Path, torn_segment: &str) -> Result<Vec<Record>, String> {
    let jdir = journal_dir(dir);
    let mut segs: Vec<_> = fs::read_dir(&jdir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("seg-") && n.ends_with(".jsonl"))
                .unwrap_or(false)
        })
        .collect();
    segs.sort();
    let mut out = Vec::new();
    let mut expect = 0u64;
    let mut prev = zeros();
    for path in segs {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name >= torn_segment {
            break;
        }
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        for line in bytes.split_inclusive(|&b| b == b'\n') {
            if line.iter().all(|b| b.is_ascii_whitespace()) {
                continue;
            }
            let rec = verify_line(line, expect, &prev).map_err(|e| format!("{e:?}"))?;
            expect = rec.seq + 1;
            prev = rec.hash.clone();
            out.push(rec);
        }
    }
    Ok(out)
}

fn peek_seq_prev(line: &[u8]) -> Option<(u64, String)> {
    let raw = line.strip_suffix(&[b'\n']).unwrap_or(line);
    let v = parse(raw, &JsonLimits::DEFAULT).ok()?;
    let seq = v.get("seq")?.as_num()?.parse().ok()?;
    let prev = v.get("prev")?.as_str()?.to_string();
    Some((seq, prev))
}

fn load_prefix(bytes: &[u8]) -> Vec<Record> {
    let mut out = Vec::new();
    let mut expect: Option<u64> = None;
    let mut prev: Option<String> = None;
    let mut start = 0;
    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|i| start + i + 1)
            .unwrap_or(bytes.len());
        let line = &bytes[start..end];
        if line.iter().all(|b| b.is_ascii_whitespace()) {
            start = end;
            continue;
        }
        let (exp, prv) = match (expect, prev.clone()) {
            (Some(e), Some(p)) => (e, p),
            _ => match peek_seq_prev(line) {
                Some(x) => x,
                None => {
                    start = end;
                    continue;
                }
            },
        };
        if let Ok(rec) = verify_line(line, exp, &prv) {
            expect = Some(rec.seq + 1);
            prev = Some(rec.hash.clone());
            out.push(rec);
        }
        start = end;
    }
    out
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
        let p = std::env::temp_dir().join(format!("fsm-j-{n}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn init_genesis_and_durability() {
        crate::clock::force_ms(5000);
        let dir = tmp();
        let mut j = init(&dir).unwrap();
        let seg = journal_dir(&dir).join("seg-00000000000000000000.jsonl");
        let bytes = fs::read(&seg).unwrap();
        assert!(bytes.ends_with(&[b'\n']));
        let rec = verify_line(&bytes, 0, &zeros()).unwrap();
        assert_eq!(rec.kind, RecordKind::Genesis);
        assert_eq!(rec.prev, zeros());
        let a = j
            .append(RecordKind::Annotated, {
                let mut b = std::collections::BTreeMap::new();
                b.insert("instance_id".into(), Value::Str("i".into()));
                Value::Obj(b)
            })
            .unwrap();
        let fresh = fs::read(&seg).unwrap();
        assert!(std::str::from_utf8(&fresh).unwrap().contains(&a.hash));
        assert!(a.ts >= 5000);
        crate::clock::reset_injected();
        crate::clock::reset_injected();
    }

    #[test]
    fn rotate_decision() {
        assert!(!should_rotate(0, 65_535));
        assert!(should_rotate(0, 65_536));
        assert!(!should_rotate(64 * 1024 * 1024 - 1, 1));
        assert!(should_rotate(64 * 1024 * 1024, 1));
    }

    #[test]
    fn poison_fast() {
        let dir = tmp();
        let mut j = init(&dir).unwrap();
        j.poisoned = true;
        let before = fs::metadata(journal_dir(&dir).join(&j.seg_name))
            .unwrap()
            .len();
        assert!(
            j.append(RecordKind::Annotated, Value::Obj(Default::default()))
                .is_err()
        );
        let after = fs::metadata(journal_dir(&dir).join(&j.seg_name))
            .unwrap()
            .len();
        assert_eq!(before, after);
    }

    #[test]
    fn lock_exclusion_and_reacquire() {
        let dir = tmp();
        let j = init(&dir).unwrap();
        let lock_path = journal_dir(&dir).join("LOCK");
        let meta = fs::read_to_string(&lock_path).unwrap();
        assert!(meta.contains(&format!("\"pid\":{}", std::process::id())));
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        assert!(f.try_lock().is_err());
        drop(j);
        let mut j2 = {
            // init would rewrite genesis; just acquire lock via open path
            let jdir = journal_dir(&dir);
            acquire_lock(&jdir).unwrap()
        };
        drop(j2);
    }

    #[test]
    fn version_marker_preflight() {
        let dir = tmp();
        let j = init(&dir).unwrap();
        drop(j);
        assert_eq!(fs::read_to_string(dir.join("VERSION")).unwrap().trim(), "5");
        fs::write(dir.join("VERSION"), "3\n").unwrap();
        assert!(matches!(
            require_store_format(&dir),
            Err(JournalHealth::VersionMismatch { found }) if found == "3"
        ));
        assert!(init(&dir).is_err());
        fs::remove_file(dir.join("VERSION")).unwrap();
        assert!(matches!(
            require_store_format(&dir),
            Err(JournalHealth::VersionMismatch { .. })
        ));
        assert!(matches!(
            init(&dir),
            Err(JournalIoError::VersionMismatch { .. })
        ));
    }

    #[test]
    fn write_store_version_refuses_missing_marker_with_journal() {
        let dir = tmp();
        let j = init(&dir).unwrap();
        drop(j);
        fs::remove_file(dir.join("VERSION")).unwrap();
        let err = write_store_version(&dir).unwrap_err();
        assert!(err.contains("store/version_mismatch"), "{err}");
    }

    #[test]
    fn clock_injected() {
        crate::clock::force_ms(100);
        assert_eq!(crate::clock::now_ms(), 100);
        assert_eq!(crate::clock::now_ms(), 101);
        crate::clock::reset_injected();
    }

    #[test]
    fn repair_rotated_torn_tail() {
        let dir = tmp();
        let def = parse(
            include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
            &JsonLimits::DEFAULT,
        )
        .unwrap();
        let mut s = crate::store::Store::open(&dir).unwrap();
        s.define_machine(def, false, false).unwrap();
        s.create_instance("case_review", "i1", "c1", None).unwrap();
        s.send_event("i1", "docs_ok", Value::Obj(Default::default()), "s1", None)
            .unwrap();
        s.send_event("i1", "docs_ok", Value::Obj(Default::default()), "s2", None)
            .unwrap();
        drop(s);
        let first = journal_dir(&dir).join("seg-00000000000000000000.jsonl");
        let bytes = fs::read(&first).unwrap();
        let lines: Vec<&[u8]> = bytes.split_inclusive(|&b| b == b'\n').collect();
        assert!(lines.len() >= 5);
        let mut keep = Vec::new();
        for line in &lines[..3] {
            keep.extend_from_slice(line);
        }
        let mut rest = Vec::new();
        for line in &lines[3..] {
            rest.extend_from_slice(line);
        }
        rest.extend(b"{\"seq\":\"5\"");
        fs::write(&first, keep).unwrap();
        let seg = journal_dir(&dir).join("seg-00000000000000000003.jsonl");
        fs::write(&seg, &rest).unwrap();
        match classify(&dir) {
            JournalHealth::TornTail { segment, .. } => {
                assert!(segment.contains("00000000000000000003"));
            }
            h => panic!("{h:?}"),
        }
        let r = repair_truncate_torn_tail(&dir).unwrap();
        assert_eq!(r.truncated_to_seq, 4);
        assert!(
            r.quarantined
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("tail-5")
        );
        let v = verify(&dir);
        assert!(matches!(v.health, JournalHealth::Ok), "{:?}", v.health);
        crate::store::Store::open(&dir).unwrap();
        let mut again = fs::read(&seg).unwrap();
        again.extend(b"{\"seq\":\"5\"");
        fs::write(&seg, again).unwrap();
        let r2 = repair_truncate_torn_tail(&dir).unwrap();
        assert_eq!(r2.truncated_to_seq, 4);
        assert!(r2.quarantined.exists());
        assert_ne!(r.quarantined, r2.quarantined);
        assert!(matches!(verify(&dir).health, JournalHealth::Ok));
    }
}
