//! Append-only journal segments with per-record fsync.

#![allow(clippy::all, unused)]
//!
//! Advisory `journal/LOCK` is released by the OS on process death; the pid
//! metadata is diagnostic only and is never trusted for liveness. There is
//! deliberately no stale-lock heuristic.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
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
    if let Ok(f) = File::open(dir) {
        let _ = f.sync_all();
    }
    Ok(())
}

fn seg_name(first: u64) -> String {
    format!("seg-{first:020}.jsonl")
}

pub fn init(dir: &Path) -> Result<Journal, JournalIoError> {
    let jdir = journal_dir(dir);
    fs::create_dir_all(&jdir).map_err(|e| JournalIoError::Io(e.to_string()))?;
    let lock = acquire_lock(&jdir)?;
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
    seg.write_all(&line)
        .map_err(|e| JournalIoError::Io(e.to_string()))?;
    seg.sync_all()
        .map_err(|e| JournalIoError::Io(e.to_string()))?;
    sync_dir(&jdir)?;
    Ok(Journal {
        dir: dir.to_path_buf(),
        seg,
        seg_name: name,
        seg_first_seq: 0,
        seg_bytes: line.len() as u64,
        seg_records: 1,
        last_seq: 0,
        last_hash: rec.hash,
        poisoned: false,
        _lock: lock,
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
    let mut expect_seq = 0u64;
    let mut expect_prev = zeros();
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
                    expect_seq = rec.seq + 1;
                    expect_prev = rec.hash;
                    off = end;
                    start = end;
                }
                Err(RecordError::NonCanonical { seq, .. }) => {
                    if last_seg
                        && is_last_line
                        && end == bytes.len()
                        && !bytes[start..].contains(&b'\n')
                        && start > 0
                        && bytes.get(start.saturating_sub(1)) != Some(&b'\n')
                    {
                        // fallthrough
                    }
                    if last_seg && is_last_line && !line.ends_with(&[b'\n']) {
                        return JournalHealth::TornTail {
                            segment: segn,
                            offset: start as u64,
                            bytes: (bytes.len() - start) as u64,
                        };
                    }
                    if last_seg && is_last_line {
                        return JournalHealth::TornTail {
                            segment: segn,
                            offset: start as u64,
                            bytes: (bytes.len() - start) as u64,
                        };
                    }
                    return JournalHealth::NonCanonical {
                        seq,
                        segment: segn,
                        offset: start as u64,
                    };
                }
                Err(RecordError::SeqGap { seq, expected }) => {
                    if last_seg && is_last_line && !line.contains(&b'\n') && line.len() < 20 {
                        return JournalHealth::TornTail {
                            segment: segn,
                            offset: start as u64,
                            bytes: line.len() as u64,
                        };
                    }
                    return JournalHealth::ChainBroken {
                        seq: expected,
                        segment: segn,
                        offset: start as u64,
                        expected: expected.to_string(),
                        found: seq.to_string(),
                    };
                }
                Err(RecordError::HashMismatch { seq }) => {
                    if last_seg
                        && is_last_line
                        && !bytes[end..].iter().any(|&b| b == b'\n')
                        && is_last_line
                    {
                        // interior if more records? last line hash fail is still interior if well-formed
                    }
                    if last_seg
                        && is_last_line
                        && verify_line(line, expect_seq, &expect_prev).is_err()
                        && !line.ends_with(b"\n")
                    {
                        return JournalHealth::TornTail {
                            segment: segn,
                            offset: start as u64,
                            bytes: line.len() as u64,
                        };
                    }
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
                    if last_seg && is_last_line {
                        return JournalHealth::TornTail {
                            segment: segn,
                            offset: start as u64,
                            bytes: (bytes.len() - start) as u64,
                        };
                    }
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
    JournalHealth::Ok
}

pub fn open(dir: &Path, sink: &mut impl RecordSink) -> Result<(Journal, StoreState), OpenError> {
    let jdir = journal_dir(dir);
    if !jdir.exists() {
        let j = init(dir).map_err(|e| OpenError::Io(e.to_string()))?;
        let st = StoreState::default();
        return Ok((j, st));
    }
    let health = classify(dir);
    if !matches!(health, JournalHealth::Ok) {
        return Err(OpenError::Health(health));
    }
    let lock = acquire_lock(&jdir).map_err(|e| match e {
        JournalIoError::Locked { pid } => {
            OpenError::Health(JournalHealth::LockIo(format!("locked {pid}")))
        }
        other => OpenError::Io(other.to_string()),
    })?;
    let recs = load_records(dir).map_err(OpenError::Io)?;
    let state = fold_with(recs.clone(), sink).map_err(|e| {
        OpenError::Health(JournalHealth::StateHashMismatch {
            seq: match e {
                fsm_core::replay::ReplayError::StateHashMismatch { seq, .. } => seq,
                _ => 0,
            },
        })
    })?;
    let last = recs.last();
    let name = last
        .map(|_| {
            let mut segs: Vec<_> = fs::read_dir(&jdir)
                .unwrap()
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("seg-"))
                .collect();
            segs.sort();
            segs.pop().unwrap_or_else(|| seg_name(0))
        })
        .unwrap_or_else(|| seg_name(0));
    let path = jdir.join(&name);
    let seg = OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(&path)
        .map_err(|e| OpenError::Io(e.to_string()))?;
    let meta = fs::metadata(&path).map_err(|e| OpenError::Io(e.to_string()))?;
    Ok((
        Journal {
            dir: dir.to_path_buf(),
            seg,
            seg_name: name,
            seg_first_seq: last.map(|r| /* approx */ 0).unwrap_or(0),
            seg_bytes: meta.len(),
            seg_records: recs.len() as u32,
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

pub fn verify(dir: &Path) -> VerifyReport {
    let health = classify(dir);
    let recs = load_records(dir).unwrap_or_default();
    let st = fold_with(recs.clone(), &mut NopSink).ok();
    VerifyReport {
        health,
        records: recs.len() as u64,
        machines: st.as_ref().map(|s| s.machines.len() as u64).unwrap_or(0),
        instances: st.as_ref().map(|s| s.instances.len() as u64).unwrap_or(0),
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
    let health = classify(dir);
    match health {
        JournalHealth::Ok => Err(RepairError::NothingToRepair),
        JournalHealth::TornTail {
            segment,
            offset,
            bytes,
        } => {
            let jdir = journal_dir(dir);
            let _lock = acquire_lock(&jdir).map_err(|e| RepairError::Io(e.to_string()))?;
            let path = jdir.join(&segment);
            let data = fs::read(&path).map_err(|e| RepairError::Io(e.to_string()))?;
            let tail = data[offset as usize..].to_vec();
            let qdir = jdir.join("quarantine");
            fs::create_dir_all(&qdir).map_err(|e| RepairError::Io(e.to_string()))?;
            let recs = load_prefix(&data[..offset as usize]);
            let first_bad = recs.last().map(|r| r.seq + 1).unwrap_or(0);
            let qpath = qdir.join(format!("{segment}-tail-{first_bad}.bin"));
            fs::write(&qpath, &tail).map_err(|e| RepairError::Io(e.to_string()))?;
            let mut f = OpenOptions::new()
                .write(true)
                .open(&path)
                .map_err(|e| RepairError::Io(e.to_string()))?;
            f.set_len(offset)
                .map_err(|e| RepairError::Io(e.to_string()))?;
            f.sync_all().map_err(|e| RepairError::Io(e.to_string()))?;
            Ok(RepairReport {
                quarantined: qpath,
                bytes,
                truncated_to_seq: recs.last().map(|r| r.seq).unwrap_or(0),
            })
        }
        other => Err(RepairError::Interior(other)),
    }
}

fn load_prefix(bytes: &[u8]) -> Vec<Record> {
    let mut out = Vec::new();
    let mut expect = 0u64;
    let mut prev = zeros();
    let mut start = 0;
    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|i| start + i + 1)
            .unwrap_or(bytes.len());
        if let Ok(rec) = verify_line(&bytes[start..end], expect, &prev) {
            expect = rec.seq + 1;
            prev = rec.hash.clone();
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
    fn clock_injected() {
        crate::clock::force_ms(100);
        assert_eq!(crate::clock::now_ms(), 100);
        assert_eq!(crate::clock::now_ms(), 101);
        crate::clock::reset_injected();
    }
}
