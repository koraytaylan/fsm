//! Append-only journal segments with per-record fsync.

//! `journal/LOCK` is released by the OS on process death; the pid metadata is
//! diagnostic only and is never trusted for liveness. There is deliberately no
//! stale-lock heuristic.
//!
//! Lock semantics differ by platform and the difference is visible in one
//! place. Unix `flock` is advisory, so the pid metadata can be read while the
//! lock is held; Windows `LockFileEx` is mandatory and refuses that read. The
//! contention path already tolerates it — the pid falls back to 0 and the
//! caller still gets `Locked` — so on Windows a busy store reports that it is
//! owned without naming the owner. Exclusion itself is identical.

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::{Record, RecordError, RecordKind, limits_value, seal, verify_line, zeros};
use fsm_core::replay::{NopSink, RecordSink, StoreState, fold_with};

const ROTATE_RECORDS: u32 = 65_536;
const ROTATE_BYTES: u64 = 64 * 1024 * 1024;
// VERSION 6 adds the `state_checkpoint` record used to bind explicit
// snapshots without rewriting prior journal records. VERSION 7 adds
// `request_fp` to every record that claims a `request_id`. VERSION 8 adds
// parallel active configurations and durable deadline schedules, with explicit
// state-hash and state-root format discriminators. Formats 1–7 (and a journal
// with no VERSION marker) are best-effort migrated on open by folding the
// complete journal and stamping 8. Records are never rewritten, so legacy
// records retain their historical hash material and request-id behavior.
/// Current marker written to a store's `VERSION` file.
pub const STORE_VERSION: &str = "8";

/// On-disk store format as detected before opening. Public because store
/// diagnostics (`fsm store status`, `fsm store repair`) report it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedStoreFormat {
    Current,
    Empty,
    Migratable { found: String },
    Incompatible { found: String },
    Unreadable { err: String },
}

fn is_seg_file_name(name: &str) -> bool {
    name.starts_with("seg-") && name.ends_with(".jsonl")
}

fn journal_segment_paths(jdir: &Path) -> Result<Vec<PathBuf>, String> {
    match crate::persistence_directory_exists(jdir) {
        Ok(true) => {}
        Ok(false) => return Err(format!("journal directory {} is absent", jdir.display())),
        Err(error) => {
            return Err(format!(
                "inspect journal directory {}: {error}",
                jdir.display()
            ));
        }
    }
    let entries = fs::read_dir(jdir)
        .map_err(|error| format!("read journal directory {}: {error}", jdir.display()))?;
    let mut segments = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "read journal directory entry in {}: {error}",
                jdir.display()
            )
        })?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_seg_file_name)
        {
            segments.push(path);
        }
    }
    segments.sort();
    Ok(segments)
}

fn has_journal_segments(dir: &Path) -> Result<bool, String> {
    let jdir = journal_dir(dir);
    match crate::persistence_directory_exists(&jdir) {
        Ok(false) => Ok(false),
        Ok(true) => journal_segment_paths(&jdir).map(|segments| !segments.is_empty()),
        Err(error) => Err(format!(
            "read journal directory {}: {error}",
            jdir.display()
        )),
    }
}

fn is_migratable_version(v: &str) -> bool {
    matches!(v, "1" | "2" | "3" | "4" | "5" | "6" | "7")
}

/// Classify an on-disk store directory without opening or locking it.
pub fn detect_store_format(dir: &Path) -> DetectedStoreFormat {
    let ver = dir.join("VERSION");
    match crate::read_regular_string_capped(&ver, crate::PERSISTENCE_READ_CAP) {
        Ok(t) => {
            let t = t.trim();
            if t == STORE_VERSION {
                return DetectedStoreFormat::Current;
            }
            if is_migratable_version(t) {
                return DetectedStoreFormat::Migratable {
                    found: t.to_string(),
                };
            }
            return DetectedStoreFormat::Incompatible {
                found: t.to_string(),
            };
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return DetectedStoreFormat::Unreadable {
                err: error.to_string(),
            };
        }
    }
    match has_journal_segments(dir) {
        Ok(true) => DetectedStoreFormat::Migratable { found: "1".into() },
        Ok(false) => DetectedStoreFormat::Empty,
        Err(err) => DetectedStoreFormat::Unreadable { err },
    }
}

pub fn should_rotate(seg_bytes: u64, seg_records: u32) -> bool {
    seg_records >= ROTATE_RECORDS || seg_bytes >= ROTATE_BYTES
}

enum Seg {
    File(File),
    Memory(Vec<u8>),
    ReadOnly,
}

impl Seg {
    fn write_line(&mut self, line: &[u8]) -> std::io::Result<()> {
        match self {
            Seg::File(f) => {
                f.write_all(line)?;
                f.sync_all()
            }
            Seg::Memory(buf) => {
                buf.extend_from_slice(line);
                Ok(())
            }
            Seg::ReadOnly => Err(std::io::Error::new(
                ErrorKind::PermissionDenied,
                "journal was opened read-only",
            )),
        }
    }
}

pub struct Journal {
    pub dir: PathBuf,
    seg: Seg,
    pub seg_name: String,
    pub seg_first_seq: u64,
    pub seg_bytes: u64,
    pub seg_records: u32,
    pub last_seq: u64,
    pub last_hash: String,
    pub poisoned: bool,
    _lock: Option<File>,
    mem_records: Option<Vec<Record>>,
}

/// Failures returned by direct journal creation and append operations.
///
/// `RecordTooLarge` is part of the current persistence-boundary migration. Code
/// that matches this enum exhaustively must handle it as a refusal before any
/// segment or in-memory journal state changes.
#[derive(Debug)]
pub enum JournalIoError {
    Locked {
        pid: i64,
    },
    Io(String),
    Poisoned,
    Record(RecordError),
    /// The canonical record envelope, excluding its terminating LF, exceeds
    /// the largest JSON persistence unit that this build can read back.
    RecordTooLarge {
        bytes: usize,
        max_bytes: usize,
    },
    VersionMismatch {
        found: String,
    },
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
            JournalIoError::RecordTooLarge { bytes, max_bytes } => {
                write!(
                    f,
                    "journal record is {bytes} bytes; the limit is {max_bytes} bytes"
                )
            }
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
    StoreIo(String),
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
                "journal has no genesis record; restore the journal from backup or recreate the data directory"
                    .into()
            }
            JournalHealth::VersionMismatch { found } => {
                let shown = if found.is_empty() { "unknown" } else { found };
                format!(
                    "store format {shown} is not supported by this build; upgrade fsm, or recreate the data directory"
                )
            }
            JournalHealth::StoreIo(s) => s.clone(),
        }
    }
}

#[derive(Debug)]
/// Failure to open a persistent journal for reading or writing.
pub enum OpenError {
    /// Authenticated content or recovery health prevents the open.
    Health(JournalHealth),
    /// A persistence input could not be inspected or read.
    ReadIo(String),
    /// A persistence destination could not be created or updated.
    WriteIo(String),
}

pub fn journal_dir(data: &Path) -> PathBuf {
    data.join("journal")
}

/// Attach the operation and path to an IO failure.
///
/// `io::Error` alone renders as bare OS text — "Access is denied. (os error 5)"
/// names neither the file nor what was attempted, which leaves an operator, and
/// anyone reading a CI log, with nothing to act on. Every IO failure on the
/// open path carries both.
fn io_err(op: &str, path: &Path, e: impl std::fmt::Display) -> JournalIoError {
    JournalIoError::Io(format!("{op} {}: {e}", path.display()))
}

fn acquire_lock(jdir: &Path) -> Result<File, JournalIoError> {
    crate::ensure_persistence_directory(jdir).map_err(|e| io_err("create journal dir", jdir, e))?;
    let path = jdir.join("LOCK");
    let mut f = crate::open_regular_file_for_write(
        &path,
        crate::PersistenceCreate::CreateIfMissing,
        crate::PersistenceWriteMode::Update,
    )
    .map_err(|e| io_err("open lock", &path, e))?;
    if f.try_lock().is_err() {
        let buf = crate::read_open_file_capped(&mut f, &path, crate::PERSISTENCE_READ_CAP)
            .unwrap_or_default();
        let pid = parse(&buf, &JsonLimits::DEFAULT)
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
        .map_err(|e| io_err("truncate lock", &path, e))?;
    let pid = std::process::id();
    let ts = crate::clock::now_ms();
    let line = format!("{{\"pid\":{pid},\"started_ts\":{ts}}}\n");
    f.write_all(line.as_bytes())
        .map_err(|e| io_err("write lock", &path, e))?;
    f.sync_all().map_err(|e| io_err("sync lock", &path, e))?;
    Ok(f)
}

fn sync_dir(dir: &Path) -> Result<(), JournalIoError> {
    crate::sync_dir(dir).map_err(|e| io_err("sync dir", dir, e))
}

fn seg_name(first: u64) -> String {
    format!("seg-{first:020}.jsonl")
}

fn write_genesis_unlocked(jdir: &Path) -> Result<(), JournalIoError> {
    crate::ensure_persistence_directory(jdir).map_err(|e| JournalIoError::Io(e.to_string()))?;
    match classify(jdir.parent().unwrap_or(jdir)) {
        JournalHealth::Ok => return Ok(()),
        JournalHealth::MissingGenesis => {}
        health => return Err(from_health(health)),
    }
    let name = seg_name(0);
    let path = jdir.join(&name);
    let mut seg = crate::open_regular_file_for_write(
        &path,
        crate::PersistenceCreate::CreateIfMissing,
        crate::PersistenceWriteMode::Append,
    )
    .map_err(|e| io_err("open genesis segment", &path, e))?;
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
    if seg
        .metadata()
        .map_err(|error| io_err("inspect genesis segment", &path, error))?
        .len()
        != 0
    {
        return Ok(());
    }
    seg.write_all(&line)
        .map_err(|e| JournalIoError::Io(e.to_string()))?;
    seg.sync_all()
        .map_err(|e| JournalIoError::Io(e.to_string()))?;
    sync_dir(jdir)?;
    Ok(())
}

fn from_health(h: JournalHealth) -> JournalIoError {
    match h {
        JournalHealth::VersionMismatch { found } => JournalIoError::VersionMismatch { found },
        other => JournalIoError::Io(other.message()),
    }
}

fn stamp_store_version(dir: &Path) -> Result<(), JournalIoError> {
    fs::create_dir_all(dir).map_err(|e| JournalIoError::Io(e.to_string()))?;
    let ver = dir.join("VERSION");
    let tmp = dir.join(format!(
        "VERSION.{}.{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    crate::write_durable(&tmp, format!("{STORE_VERSION}\n").as_bytes())
        .map_err(|e| io_err("write", &tmp, e))?;
    match fs::rename(&tmp, &ver) {
        Ok(()) => {}
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            if ver.exists()
                && crate::read_regular_string_capped(&ver, crate::PERSISTENCE_READ_CAP)
                    .map(|s| s.trim() == STORE_VERSION)
                    .unwrap_or(false)
            {
                return Ok(());
            }
            return Err(JournalIoError::Io(e.to_string()));
        }
    }
    sync_dir(dir)
}

fn write_version_durable(dir: &Path) -> Result<(), JournalIoError> {
    let version = dir.join("VERSION");
    match fs::symlink_metadata(&version) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.file_type().is_file() => {
            return Err(io_err(
                "inspect VERSION",
                &version,
                "VERSION must be a regular, non-symlink file",
            ));
        }
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(io_err("inspect VERSION", &version, error)),
    }
    stamp_store_version(dir)
}

pub fn init(dir: &Path) -> Result<Journal, JournalIoError> {
    if let Err(h) = refuse_incompatible_store_format(dir) {
        return Err(from_health(h));
    }
    let jdir = journal_dir(dir);
    crate::ensure_persistence_directory(&jdir)
        .map_err(|e| io_err("create journal dir", &jdir, e))?;
    let lock = acquire_lock(&jdir)?;
    if let Err(h) = refuse_incompatible_store_format(dir) {
        drop(lock);
        return Err(from_health(h));
    }
    let fmt = detect_store_format(dir);
    if matches!(fmt, DetectedStoreFormat::Empty) {
        write_version_durable(dir)?;
    }
    // A Migratable dir must keep its own journal; writing a fresh genesis
    // there would orphan the old data behind a new chain.
    if matches!(
        fmt,
        DetectedStoreFormat::Empty | DetectedStoreFormat::Current
    ) {
        write_genesis_unlocked(&jdir)?;
    }
    drop(lock);
    let mut sink = fsm_core::replay::NopSink;
    open(dir, &mut sink)
        .map(|(j, _, _)| j)
        .map_err(|e| match e {
            OpenError::ReadIo(s) | OpenError::WriteIo(s) => JournalIoError::Io(s),
            OpenError::Health(h) => JournalIoError::Io(h.message()),
        })
}

impl Journal {
    pub fn memory() -> Self {
        let mut body = std::collections::BTreeMap::new();
        body.insert("format".into(), Value::Str("fsm.journal/1".into()));
        body.insert("created_ts".into(), Value::Num("0".into()));
        body.insert("limits".into(), limits_value());
        let rec = seal(0, 0, RecordKind::Genesis, Value::Obj(body), &zeros());
        let line = rec.to_line();
        Journal {
            dir: PathBuf::from("<memory>"),
            seg: Seg::Memory(line),
            seg_name: "mem".into(),
            seg_first_seq: 0,
            seg_bytes: 0,
            seg_records: 1,
            last_seq: 0,
            last_hash: rec.hash.clone(),
            poisoned: false,
            _lock: None,
            mem_records: Some(vec![rec]),
        }
    }

    pub fn is_memory(&self) -> bool {
        self.mem_records.is_some()
    }

    /// Return whether this journal was opened for inspection only.
    pub fn is_read_only(&self) -> bool {
        matches!(self.seg, Seg::ReadOnly)
    }

    pub fn memory_records(&self) -> Option<&[Record]> {
        self.mem_records.as_deref()
    }

    pub fn append(&mut self, kind: RecordKind, body: Value) -> Result<Record, JournalIoError> {
        self.append_at(kind, body, crate::clock::now_ms())
    }

    pub fn append_at(
        &mut self,
        kind: RecordKind,
        body: Value,
        ts: i64,
    ) -> Result<Record, JournalIoError> {
        if self.poisoned {
            return Err(JournalIoError::Poisoned);
        }
        let rec = seal(self.last_seq + 1, ts, kind, body, &self.last_hash);
        let line = rec.to_line();
        // `Record::to_line` always appends exactly one LF; the streaming reader
        // applies its cap to the bytes before that delimiter.
        let record_bytes = line.len() - 1;
        if record_bytes > crate::PERSISTENCE_READ_CAP {
            return Err(JournalIoError::RecordTooLarge {
                bytes: record_bytes,
                max_bytes: crate::PERSISTENCE_READ_CAP,
            });
        }
        if !self.is_memory() && should_rotate(self.seg_bytes, self.seg_records) {
            if let Err(e) = self.rotate() {
                self.poisoned = true;
                return Err(e);
            }
        }
        if let Err(e) = self.seg.write_line(&line) {
            self.poisoned = true;
            return Err(JournalIoError::Io(e.to_string()));
        }
        self.seg_bytes += line.len() as u64;
        self.seg_records += 1;
        self.last_seq = rec.seq;
        self.last_hash = rec.hash.clone();
        if let Some(recs) = &mut self.mem_records {
            recs.push(rec.clone());
        }
        Ok(rec)
    }

    fn rotate(&mut self) -> Result<(), JournalIoError> {
        if self.is_read_only() {
            return Err(JournalIoError::Io("journal was opened read-only".into()));
        }
        if self.is_memory() {
            return Ok(());
        }
        let next = self.last_seq + 1;
        let name = seg_name(next);
        let directory = journal_dir(&self.dir);
        crate::ensure_persistence_directory(&directory)
            .map_err(|e| JournalIoError::Io(e.to_string()))?;
        let path = directory.join(&name);
        let seg = crate::open_regular_file_for_write(
            &path,
            crate::PersistenceCreate::CreateIfMissing,
            crate::PersistenceWriteMode::Append,
        )
        .map_err(|e| JournalIoError::Io(e.to_string()))?;
        sync_dir(&directory)?;
        self.seg = Seg::File(seg);
        self.seg_name = name;
        self.seg_first_seq = next;
        self.seg_bytes = 0;
        self.seg_records = 0;
        Ok(())
    }

    /// Close the current segment and open the next one. Tests use this to
    /// produce multiple on-disk segments without writing `ROTATE_RECORDS`.
    pub fn force_rotate(&mut self) -> Result<(), JournalIoError> {
        self.rotate()
    }
}

pub fn classify(dir: &Path) -> JournalHealth {
    let jdir = journal_dir(dir);
    let segs = match journal_segment_paths(&jdir) {
        Ok(segments) => segments,
        Err(error) => return JournalHealth::StoreIo(error),
    };
    if segs.is_empty() {
        return JournalHealth::MissingGenesis;
    }
    let mut expect_seq = 0u64;
    let mut expect_prev = zeros();
    let mut saw_record = false;
    for (si, path) in segs.iter().enumerate() {
        let last_seg = si + 1 == segs.len();
        let segn = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut reader = match crate::CappedLineReader::open(path, crate::PERSISTENCE_READ_CAP) {
            Ok(reader) => reader,
            Err(error) => {
                return JournalHealth::StoreIo(format!(
                    "read journal segment {}: {error}",
                    path.display()
                ));
            }
        };
        loop {
            let line = match reader.next_line() {
                Ok(Some(line)) => line,
                Ok(None) => break,
                Err(error) => {
                    return JournalHealth::StoreIo(format!(
                        "read journal segment {}: {error}",
                        path.display()
                    ));
                }
            };
            if !line.terminated {
                if last_seg {
                    return JournalHealth::TornTail {
                        segment: segn.clone(),
                        offset: line.start,
                        bytes: line.end.saturating_sub(line.start),
                    };
                }
                return JournalHealth::ChainBroken {
                    seq: expect_seq,
                    segment: segn.clone(),
                    offset: line.start,
                    expected: expect_prev.clone(),
                    found: "unterminated record in a non-final segment".into(),
                };
            }
            match verify_line(&line.bytes, expect_seq, &expect_prev) {
                Ok(rec) => {
                    if rec.seq == 0 && rec.kind != RecordKind::Genesis {
                        return JournalHealth::ChainBroken {
                            seq: 0,
                            segment: segn.clone(),
                            offset: line.start,
                            expected: "genesis".into(),
                            found: format!("{:?}", rec.kind),
                        };
                    }
                    expect_seq = rec.seq + 1;
                    expect_prev = rec.hash;
                    saw_record = true;
                }
                Err(RecordError::NonCanonical { seq, .. }) => {
                    return JournalHealth::NonCanonical {
                        seq,
                        segment: segn.clone(),
                        offset: line.start,
                    };
                }
                Err(RecordError::SeqGap { seq, expected }) => {
                    return JournalHealth::ChainBroken {
                        seq: expected,
                        segment: segn.clone(),
                        offset: line.start,
                        expected: expected.to_string(),
                        found: seq.to_string(),
                    };
                }
                Err(RecordError::HashMismatch { seq }) => {
                    return JournalHealth::ChainBroken {
                        seq,
                        segment: segn.clone(),
                        offset: line.start,
                        expected: expect_prev.clone(),
                        found: "hash".into(),
                    };
                }
                Err(RecordError::Parse { .. })
                | Err(RecordError::PrevMismatch { .. })
                | Err(RecordError::BodyInvalid { .. }) => {
                    return JournalHealth::ChainBroken {
                        seq: expect_seq,
                        segment: segn.clone(),
                        offset: line.start,
                        expected: expect_prev.clone(),
                        found: "parse".into(),
                    };
                }
            }
        }
    }
    if !saw_record {
        return JournalHealth::MissingGenesis;
    }
    JournalHealth::Ok
}

pub(crate) fn replay_health(e: fsm_core::replay::ReplayError) -> JournalHealth {
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

pub fn open(
    dir: &Path,
    sink: &mut impl RecordSink,
) -> Result<(Journal, StoreState, crate::snapshot::OpenPath), OpenError> {
    let jdir = journal_dir(dir);
    // Existing output directories are validated as writer destinations before
    // format probing. Missing directories are not created until an existing
    // incompatible VERSION marker has been refused without mutation.
    crate::persistence_directory_exists(&jdir)
        .map_err(|error| OpenError::WriteIo(error.to_string()))?;
    if let Err(h) = refuse_incompatible_store_format(dir) {
        return Err(OpenError::Health(h));
    }
    crate::ensure_persistence_directory(&jdir)
        .map_err(|error| OpenError::WriteIo(error.to_string()))?;
    let lock = acquire_lock(&jdir).map_err(|e| match e {
        JournalIoError::Locked { pid } => {
            OpenError::Health(JournalHealth::LockIo(format!("locked {pid}")))
        }
        other => OpenError::WriteIo(other.to_string()),
    })?;
    if let Err(h) = refuse_incompatible_store_format(dir) {
        return Err(OpenError::Health(h));
    }
    let fmt = detect_store_format(dir);
    if matches!(fmt, DetectedStoreFormat::Empty) {
        write_version_durable(dir).map_err(|e| OpenError::WriteIo(e.to_string()))?;
    }
    let migrating = matches!(fmt, DetectedStoreFormat::Migratable { .. });
    let health = classify(dir);
    if matches!(health, JournalHealth::MissingGenesis) {
        // Auto-genesis only completes a store this build created (fresh dir,
        // or a crash between VERSION and genesis). A Migratable dir missing
        // its journal is lost data, not a store to re-create over.
        if migrating {
            return Err(OpenError::Health(JournalHealth::MissingGenesis));
        }
        write_genesis_unlocked(&jdir).map_err(|e| OpenError::WriteIo(e.to_string()))?;
    }
    let health = classify(dir);
    if !matches!(health, JournalHealth::Ok) {
        return Err(OpenError::Health(health));
    }
    let (recs, (name, first, bytes, count)) =
        load_records_with_active_meta(dir, FinalTailPolicy::Reject).map_err(OpenError::ReadIo)?;
    let (state, open_path) = if migrating {
        // Migration ignores snapshot caches and folds the complete journal
        // under current semantics before certifying the store.
        let n = recs.len();
        let state =
            fold_with(recs.clone(), sink).map_err(|e| OpenError::Health(replay_health(e)))?;
        (
            state,
            crate::snapshot::OpenPath {
                replayed_records: n,
                used_snapshot: false,
                snapshot_seq: None,
            },
        )
    } else {
        crate::snapshot::open_state(dir, recs.clone(), sink)
            .map_err(|e| OpenError::Health(replay_health(e)))?
    };
    if migrating {
        stamp_store_version(dir).map_err(|e| OpenError::WriteIo(e.to_string()))?;
    }
    let last = recs.last();
    let path = jdir.join(&name);
    let seg = crate::open_regular_file_for_write(
        &path,
        crate::PersistenceCreate::CreateIfMissing,
        crate::PersistenceWriteMode::Append,
    )
    .map_err(|e| OpenError::WriteIo(e.to_string()))?;
    Ok((
        Journal {
            dir: dir.to_path_buf(),
            seg: Seg::File(seg),
            seg_name: name,
            seg_first_seq: first,
            seg_bytes: bytes,
            seg_records: count,
            last_seq: last.map(|r| r.seq).unwrap_or(0),
            last_hash: last.map(|r| r.hash.clone()).unwrap_or_else(zeros),
            poisoned: false,
            _lock: Some(lock),
            mem_records: None,
        },
        state,
        open_path,
    ))
}

/// Open a store for inspection without creating anything, taking the writer
/// lock, stamping a migrated VERSION, or opening a segment for append.
pub(crate) fn open_read_only(
    dir: &Path,
    sink: &mut impl RecordSink,
) -> Result<(Journal, StoreState, crate::snapshot::OpenPath, Vec<Record>), OpenError> {
    let format = detect_store_format(dir);
    if matches!(format, DetectedStoreFormat::Empty) {
        return Ok((
            Journal {
                dir: dir.to_path_buf(),
                seg: Seg::ReadOnly,
                seg_name: seg_name(0),
                seg_first_seq: 0,
                seg_bytes: 0,
                seg_records: 0,
                last_seq: 0,
                last_hash: zeros(),
                poisoned: false,
                _lock: None,
                mem_records: None,
            },
            StoreState::default(),
            crate::snapshot::OpenPath::default(),
            Vec::new(),
        ));
    }
    if let Err(health) = refuse_incompatible_store_format(dir) {
        return Err(OpenError::Health(health));
    }
    let health = classify(dir);
    if !matches!(health, JournalHealth::Ok | JournalHealth::TornTail { .. }) {
        return Err(OpenError::Health(health));
    }
    let (records, (name, first, bytes, count)) =
        load_records_with_active_meta(dir, FinalTailPolicy::Ignore).map_err(OpenError::ReadIo)?;
    let migrating = matches!(format, DetectedStoreFormat::Migratable { .. });
    let (state, open_path) = if migrating {
        let count = records.len();
        let state = fold_with(records.clone(), sink)
            .map_err(|error| OpenError::Health(replay_health(error)))?;
        (
            state,
            crate::snapshot::OpenPath {
                replayed_records: count,
                used_snapshot: false,
                snapshot_seq: None,
            },
        )
    } else {
        crate::snapshot::open_state_read_only(dir, records.clone(), sink)
            .map_err(|error| OpenError::Health(replay_health(error)))?
    };
    let last = records.last();
    Ok((
        Journal {
            dir: dir.to_path_buf(),
            seg: Seg::ReadOnly,
            seg_name: name,
            seg_first_seq: first,
            seg_bytes: bytes,
            seg_records: count,
            last_seq: last.map(|record| record.seq).unwrap_or(0),
            last_hash: last.map(|record| record.hash.clone()).unwrap_or_else(zeros),
            poisoned: false,
            _lock: None,
            mem_records: None,
        },
        state,
        open_path,
        records,
    ))
}

pub fn load_records(dir: &Path) -> Result<Vec<Record>, String> {
    load_records_with_active_meta(dir, FinalTailPolicy::Reject).map(|(records, _)| records)
}

type ActiveSegmentMeta = (String, u64, u64, u32);

#[derive(Clone, Copy, PartialEq, Eq)]
enum FinalTailPolicy {
    Reject,
    Ignore,
}

/// Load and verify one authoritative record prefix while deriving its active
/// segment metadata from those same bounded reads. A read-only caller must not
/// re-list or re-stat the journal afterward: a live writer may advance between
/// independent observations.
fn load_records_with_active_meta(
    dir: &Path,
    final_tail_policy: FinalTailPolicy,
) -> Result<(Vec<Record>, ActiveSegmentMeta), String> {
    let jdir = journal_dir(dir);
    let segs = journal_segment_paths(&jdir)?;
    let mut out = Vec::new();
    let mut expect = 0u64;
    let mut prev = zeros();
    let mut active = (seg_name(0), 0, 0, 0);
    for (index, path) in segs.iter().enumerate() {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| seg_name(0));
        let first = name
            .strip_prefix("seg-")
            .and_then(|value| value.strip_suffix(".jsonl"))
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let mut segment_records = 0u32;
        let mut visible_bytes = None;
        let mut reader = crate::CappedLineReader::open(path, crate::PERSISTENCE_READ_CAP)
            .map_err(|error| format!("read journal segment {}: {error}", path.display()))?;
        while let Some(line) = reader
            .next_line()
            .map_err(|error| format!("read journal segment {}: {error}", path.display()))?
        {
            if !line.terminated {
                if index + 1 == segs.len() && final_tail_policy == FinalTailPolicy::Ignore {
                    visible_bytes = Some(line.start);
                    break;
                }
                return Err(format!(
                    "unterminated journal record in {} at offset {}",
                    path.display(),
                    line.start
                ));
            }
            if line.bytes.iter().all(|byte| byte.is_ascii_whitespace()) {
                continue;
            }
            let rec = verify_line(&line.bytes, expect, &prev).map_err(|e| format!("{e:?}"))?;
            expect = rec.seq + 1;
            prev = rec.hash.clone();
            out.push(rec);
            segment_records = segment_records.saturating_add(1);
        }
        if index + 1 == segs.len() {
            active = (
                name,
                first,
                visible_bytes.unwrap_or_else(|| reader.position()),
                segment_records,
            );
        }
    }
    Ok((out, active))
}

#[derive(Debug, Clone)]
pub struct SegmentProgress {
    pub segment: String,
    pub records: u64,
    pub first_seq: Option<u64>,
    pub last_seq: Option<u64>,
    pub status: String,
}

pub struct VerifyReport {
    pub health: JournalHealth,
    pub records: u64,
    pub machines: u64,
    pub instances: u64,
    pub instance_hashes: std::collections::BTreeMap<String, String>,
    pub segments: Vec<SegmentProgress>,
    pub store_version: Option<String>,
    pub migratable: bool,
}

pub fn refuse_incompatible_store_format(dir: &Path) -> Result<(), JournalHealth> {
    match detect_store_format(dir) {
        DetectedStoreFormat::Incompatible { found } => {
            Err(JournalHealth::VersionMismatch { found })
        }
        DetectedStoreFormat::Unreadable { err } => Err(JournalHealth::StoreIo(format!(
            "cannot inspect store format: {err}"
        ))),
        _ => Ok(()),
    }
}

pub fn verify_segments(dir: &Path) -> Vec<SegmentProgress> {
    let jdir = journal_dir(dir);
    let segs = match journal_segment_paths(&jdir) {
        Ok(segments) => segments,
        Err(_) => {
            return vec![SegmentProgress {
                segment: "journal".into(),
                records: 0,
                first_seq: None,
                last_seq: None,
                status: "metadata-failure".into(),
            }];
        }
    };
    let mut out = Vec::new();
    let mut expect_seq = 0u64;
    let mut expect_prev = zeros();
    for (si, path) in segs.iter().enumerate() {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut reader = match crate::CappedLineReader::open(path, crate::PERSISTENCE_READ_CAP) {
            Ok(reader) => reader,
            Err(_) => {
                out.push(SegmentProgress {
                    segment: name,
                    records: 0,
                    first_seq: None,
                    last_seq: None,
                    status: "metadata-failure".into(),
                });
                continue;
            }
        };
        let last_seg = si + 1 == segs.len();
        let mut records = 0u64;
        let mut first = None;
        let mut last = None;
        let mut status = "ok".to_string();
        let mut saw_line = false;
        loop {
            let line = match reader.next_line() {
                Ok(Some(line)) => line,
                Ok(None) => break,
                Err(_) => {
                    status = "metadata-failure".into();
                    break;
                }
            };
            saw_line = true;
            if !line.terminated {
                status = if last_seg { "torn" } else { "broken" }.into();
                break;
            }
            match verify_line(&line.bytes, expect_seq, &expect_prev) {
                Ok(rec) => {
                    records += 1;
                    if first.is_none() {
                        first = Some(rec.seq);
                    }
                    last = Some(rec.seq);
                    expect_seq = rec.seq + 1;
                    expect_prev = rec.hash;
                }
                Err(_) => {
                    status = "broken".into();
                    break;
                }
            }
        }
        if records == 0 && status == "ok" && !saw_line {
            status = "empty".into();
        }
        out.push(SegmentProgress {
            segment: name,
            records,
            first_seq: first,
            last_seq: last,
            status,
        });
    }
    out
}

pub fn verify(dir: &Path) -> VerifyReport {
    let fmt = detect_store_format(dir);
    let store_version = match &fmt {
        DetectedStoreFormat::Current => Some(STORE_VERSION.to_string()),
        DetectedStoreFormat::Migratable { found } | DetectedStoreFormat::Incompatible { found } => {
            Some(found.clone())
        }
        DetectedStoreFormat::Empty | DetectedStoreFormat::Unreadable { .. } => None,
    };
    let migratable = matches!(&fmt, DetectedStoreFormat::Migratable { .. });
    let empty = |health: JournalHealth| VerifyReport {
        health,
        records: 0,
        machines: 0,
        instances: 0,
        instance_hashes: Default::default(),
        segments: verify_segments(dir),
        store_version: store_version.clone(),
        migratable,
    };
    if let Err(h) = refuse_incompatible_store_format(dir) {
        return empty(h);
    }
    let health = classify(dir);
    if !matches!(health, JournalHealth::Ok) {
        return empty(health);
    }
    let recs = match load_records(dir) {
        Ok(r) => r,
        Err(e) => return empty(JournalHealth::StoreIo(e)),
    };
    match fold_with(recs.clone(), &mut NopSink) {
        Ok(st) => {
            let mut instance_hashes = std::collections::BTreeMap::new();
            for (id, inst) in &st.instances {
                let mid = st.instance_machines.get(id).cloned().unwrap_or_default();
                instance_hashes.insert(
                    id.clone(),
                    fsm_core::hashes::state_hash(&mid, id, st.last_seq, inst),
                );
            }
            VerifyReport {
                health: JournalHealth::Ok,
                records: recs.len() as u64,
                machines: st.machines.len() as u64,
                instances: st.instances.len() as u64,
                instance_hashes,
                segments: verify_segments(dir),
                store_version,
                migratable,
            }
        }
        Err(e) => VerifyReport {
            health: replay_health(e),
            records: recs.len() as u64,
            machines: 0,
            instances: 0,
            instance_hashes: Default::default(),
            segments: verify_segments(dir),
            store_version,
            migratable,
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
/// Failure to perform the explicitly mutating torn-tail repair operation.
pub enum RepairError {
    /// The journal is healthy and needs no torn-tail repair.
    NothingToRepair,
    /// Journal content has a health problem that this repair cannot change.
    Interior(JournalHealth),
    /// Authoritative bytes could not be read before repair.
    ReadIo(String),
    /// Repair output could not be written durably.
    WriteIo(String),
}

pub fn repair_truncate_torn_tail(dir: &Path) -> Result<RepairReport, RepairError> {
    if let Err(h) = refuse_incompatible_store_format(dir) {
        return Err(RepairError::Interior(h));
    }
    let jdir = journal_dir(dir);
    let _lock = acquire_lock(&jdir).map_err(|error| match error {
        JournalIoError::Locked { pid } => {
            RepairError::Interior(JournalHealth::LockIo(format!("locked {pid}")))
        }
        other => RepairError::WriteIo(other.to_string()),
    })?;
    let migrating = matches!(
        detect_store_format(dir),
        DetectedStoreFormat::Migratable { .. }
    );
    let health = classify(dir);
    match health {
        JournalHealth::Ok => Err(RepairError::NothingToRepair),
        JournalHealth::TornTail {
            segment,
            offset,
            bytes,
        } => {
            let path = jdir.join(&segment);
            let tail =
                crate::read_regular_range_capped(&path, offset, bytes, crate::PERSISTENCE_READ_CAP)
                    .map_err(|error| RepairError::ReadIo(error.to_string()))?;
            let chain =
                load_records_before_offset(dir, &segment, offset).map_err(RepairError::ReadIo)?;
            if let Err(e) = fold_with(chain.clone(), &mut NopSink) {
                return Err(RepairError::Interior(replay_health(e)));
            }
            let qdir = jdir.join("quarantine");
            crate::ensure_persistence_directory(&qdir)
                .map_err(|e| RepairError::WriteIo(e.to_string()))?;
            sync_dir(&jdir).map_err(|e| RepairError::WriteIo(e.to_string()))?;
            let seg_first = segment
                .strip_prefix("seg-")
                .and_then(|s| s.strip_suffix(".jsonl"))
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let first_bad = chain.last().map(|r| r.seq + 1).unwrap_or(seg_first);
            let mut qpath = qdir.join(format!("{segment}-tail-{first_bad}.bin"));
            let qf = match OpenOptions::new().write(true).create_new(true).open(&qpath) {
                Ok(file) => file,
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    let nonce = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.as_nanos())
                        .unwrap_or(0);
                    qpath = qdir.join(format!(
                        "{segment}-tail-{first_bad}-{}-{nonce}.bin",
                        std::process::id()
                    ));
                    OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&qpath)
                        .map_err(|error| RepairError::WriteIo(error.to_string()))?
                }
                Err(error) => return Err(RepairError::WriteIo(error.to_string())),
            };
            {
                let mut qf = qf;
                qf.write_all(&tail)
                    .map_err(|e| RepairError::WriteIo(e.to_string()))?;
                qf.sync_all()
                    .map_err(|e| RepairError::WriteIo(e.to_string()))?;
            }
            sync_dir(&qdir).map_err(|e| RepairError::WriteIo(e.to_string()))?;
            sync_dir(&jdir).map_err(|e| RepairError::WriteIo(e.to_string()))?;
            let f = crate::open_regular_file_for_write(
                &path,
                crate::PersistenceCreate::RequireExisting,
                crate::PersistenceWriteMode::Update,
            )
            .map_err(|e| RepairError::WriteIo(e.to_string()))?;
            f.set_len(offset)
                .map_err(|e| RepairError::WriteIo(e.to_string()))?;
            f.sync_all()
                .map_err(|e| RepairError::WriteIo(e.to_string()))?;
            sync_dir(&jdir).map_err(|e| RepairError::WriteIo(e.to_string()))?;
            if offset == 0 && seg_first == 0 {
                write_genesis_unlocked(&jdir).map_err(|e| RepairError::WriteIo(e.to_string()))?;
            }
            let after = classify(dir);
            if !matches!(after, JournalHealth::Ok) {
                return Err(RepairError::Interior(after));
            }
            let kept = load_records(dir).map_err(RepairError::ReadIo)?;
            if let Err(e) = fold_with(kept, &mut NopSink) {
                return Err(RepairError::Interior(replay_health(e)));
            }
            if migrating {
                // A successful repair has folded the complete retained journal
                // under current semantics — the migration success condition.
                stamp_store_version(dir).map_err(|e| RepairError::WriteIo(e.to_string()))?;
            }
            Ok(RepairReport {
                quarantined: qpath,
                bytes,
                truncated_to_seq: chain
                    .last()
                    .map(|r| r.seq)
                    .unwrap_or(seg_first.saturating_sub(1)),
            })
        }
        other => Err(RepairError::Interior(other)),
    }
}

fn load_records_before_offset(
    dir: &Path,
    torn_segment: &str,
    offset: u64,
) -> Result<Vec<Record>, String> {
    let jdir = journal_dir(dir);
    let segs = journal_segment_paths(&jdir)?;
    let mut out = Vec::new();
    let mut expect = 0u64;
    let mut prev = zeros();
    for path in segs {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name > torn_segment {
            break;
        }
        let is_torn = name == torn_segment;
        let mut reader = crate::CappedLineReader::open(&path, crate::PERSISTENCE_READ_CAP)
            .map_err(|error| format!("read journal segment {}: {error}", path.display()))?;
        loop {
            if is_torn && reader.position() >= offset {
                break;
            }
            let Some(line) = reader
                .next_line()
                .map_err(|error| format!("read journal segment {}: {error}", path.display()))?
            else {
                break;
            };
            if is_torn && line.end > offset {
                return Err("stale torn-tail offset".into());
            }
            if line.bytes.iter().all(|byte| byte.is_ascii_whitespace()) {
                continue;
            }
            let rec = verify_line(&line.bytes, expect, &prev).map_err(|e| format!("{e:?}"))?;
            expect = rec.seq + 1;
            prev = rec.hash.clone();
            out.push(rec);
        }
    }
    Ok(out)
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
        let p = std::env::temp_dir().join(format!("fsm-j-{pid}-{n}-{i}"));
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
        assert!(bytes.ends_with(b"\n"));
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
        // Readable while held only where locks are advisory. Windows refuses
        // this read outright, which is why the pid is diagnostic and the
        // contention path never depends on it.
        #[cfg(unix)]
        {
            let meta = fs::read_to_string(&lock_path).unwrap();
            assert!(meta.contains(&format!("\"pid\":{}", std::process::id())));
        }
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        assert!(f.try_lock().is_err());
        drop(j);
        let j2 = {
            // init would rewrite genesis; just acquire lock via open path
            let jdir = journal_dir(&dir);
            acquire_lock(&jdir).unwrap()
        };
        drop(j2);
    }

    #[test]
    fn read_only_open_returns_the_exact_folded_record_prefix() {
        let dir = tmp();
        let mut writer = crate::store::Store::open(&dir).unwrap();
        let definition = parse(
            include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
            &JsonLimits::DEFAULT,
        )
        .unwrap();
        writer.define_machine(definition, false, false).unwrap();

        let mut sink = NopSink;
        let (reader, state, open_path, records) = open_read_only(&dir, &mut sink).unwrap();
        let returned_last_seq = records.last().map(|record| record.seq).unwrap_or(0);
        assert_eq!(returned_last_seq, reader.last_seq);
        assert_eq!(returned_last_seq, state.last_seq);
        assert_eq!(open_path.replayed_records, records.len());
        assert_eq!(reader.seg_records as usize, records.len());
        let returned_segment = dir.join("journal").join(&reader.seg_name);
        assert_eq!(
            fs::metadata(&returned_segment).unwrap().len(),
            reader.seg_bytes
        );
        let returned_segment_bytes = reader.seg_bytes;

        // A writer may advance immediately after the bounded read. The
        // returned vector remains the authoritative prefix for both the
        // journal metadata and folded state assembled above.
        writer
            .create_instance("case_review", "later-instance", "create-later", None)
            .unwrap();
        assert!(load_records(&dir).unwrap().last().unwrap().seq > returned_last_seq);
        assert_eq!(records.last().unwrap().seq, state.last_seq);
        assert!(
            fs::metadata(returned_segment).unwrap().len() > returned_segment_bytes,
            "the live writer should advance the same segment after the read-only prefix"
        );
        assert_eq!(reader.seg_bytes, returned_segment_bytes);
    }

    #[test]
    fn version_marker_preflight() {
        let dir = tmp();
        let j = init(&dir).unwrap();
        drop(j);
        assert_eq!(
            fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
            STORE_VERSION
        );
        fs::write(dir.join("VERSION"), "3\n").unwrap();
        assert_eq!(
            detect_store_format(&dir),
            DetectedStoreFormat::Migratable { found: "3".into() }
        );
        assert!(refuse_incompatible_store_format(&dir).is_ok());
        let j = init(&dir).unwrap();
        drop(j);
        assert_eq!(
            fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
            STORE_VERSION
        );
        fs::remove_file(dir.join("VERSION")).unwrap();
        assert_eq!(
            detect_store_format(&dir),
            DetectedStoreFormat::Migratable { found: "1".into() }
        );
        let j = init(&dir).unwrap();
        drop(j);
        assert_eq!(
            fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
            STORE_VERSION
        );
        // One past the current format: written by a newer build, so it is
        // refused rather than migrated. Derived so a version bump keeps this a
        // future version instead of silently testing the current one.
        let future = (STORE_VERSION.parse::<u32>().unwrap() + 1).to_string();
        fs::write(dir.join("VERSION"), format!("{future}\n")).unwrap();
        assert!(matches!(
            refuse_incompatible_store_format(&dir),
            Err(JournalHealth::VersionMismatch { found }) if found == future
        ));
        assert!(matches!(
            init(&dir),
            Err(JournalIoError::VersionMismatch { found }) if found == future
        ));
    }

    #[test]
    fn migratable_marker_stamps_after_successful_open() {
        let dir = tmp();
        let j = init(&dir).unwrap();
        drop(j);
        fs::write(dir.join("VERSION"), "5\n").unwrap();
        let mut sink = NopSink;
        drop(open(&dir, &mut sink).unwrap());
        assert_eq!(
            fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
            STORE_VERSION
        );
        fs::remove_file(dir.join("VERSION")).unwrap();
        drop(open(&dir, &mut sink).unwrap());
        assert_eq!(
            fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
            STORE_VERSION
        );
    }

    #[test]
    fn every_prior_version_migrates_after_successful_full_fold() {
        for prior in 1..STORE_VERSION.parse::<u32>().unwrap() {
            let dir = tmp();
            let journal = init(&dir).unwrap();
            drop(journal);
            fs::write(dir.join("VERSION"), format!("{prior}\n")).unwrap();
            assert_eq!(
                detect_store_format(&dir),
                DetectedStoreFormat::Migratable {
                    found: prior.to_string()
                }
            );

            let mut sink = NopSink;
            drop(open(&dir, &mut sink).unwrap());
            assert_eq!(
                fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
                STORE_VERSION
            );
        }
    }

    #[test]
    fn version_seven_migrates_with_the_exact_historical_genesis_limits() {
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        let cleanup = Cleanup(tmp());
        let dir = &cleanup.0;
        let journal = init(dir).unwrap();
        drop(journal);

        let current = load_records(dir).unwrap().remove(0);
        let Value::Obj(mut body) = current.body else {
            panic!("genesis body must be an object")
        };
        let Value::Obj(mut limits) = body.remove("limits").unwrap() else {
            panic!("genesis limits must be an object")
        };
        limits.remove("max_regions");
        limits.remove("max_deadlines");
        limits.remove("max_eval_ticks");
        body.insert("limits".into(), Value::Obj(limits));
        let legacy = seal(
            0,
            current.ts,
            RecordKind::Genesis,
            Value::Obj(body),
            &zeros(),
        );
        fs::write(
            journal_dir(dir).join("seg-00000000000000000000.jsonl"),
            legacy.to_line(),
        )
        .unwrap();
        fs::write(dir.join("VERSION"), "7\n").unwrap();

        let mut sink = NopSink;
        let (reopened, state, path) = open(dir, &mut sink).unwrap();
        assert!(state.instances.is_empty());
        assert_eq!(path.replayed_records, 1);
        assert!(!path.used_snapshot);
        drop(reopened);
        assert_eq!(
            fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
            STORE_VERSION
        );
    }

    #[test]
    fn migratable_torn_tail_does_not_stamp() {
        let dir = tmp();
        let j = init(&dir).unwrap();
        drop(j);
        let seg = journal_dir(&dir).join("seg-00000000000000000000.jsonl");
        let mut bytes = fs::read(&seg).unwrap();
        bytes.extend_from_slice(b"{\"partial");
        fs::write(&seg, bytes).unwrap();
        fs::write(dir.join("VERSION"), "5\n").unwrap();
        let mut sink = NopSink;
        assert!(matches!(
            open(&dir, &mut sink),
            Err(OpenError::Health(JournalHealth::TornTail { .. }))
        ));
        assert_eq!(fs::read_to_string(dir.join("VERSION")).unwrap().trim(), "5");
    }

    #[test]
    fn migratable_marker_without_journal_refuses() {
        let dir = tmp();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("VERSION"), "3\n").unwrap();
        let mut sink = NopSink;
        assert!(matches!(
            open(&dir, &mut sink),
            Err(OpenError::Health(JournalHealth::MissingGenesis))
        ));
        assert!(matches!(init(&dir), Err(JournalIoError::Io(_))));
        assert_eq!(fs::read_to_string(dir.join("VERSION")).unwrap().trim(), "3");
    }

    #[test]
    fn stray_tmp_segment_is_not_a_store() {
        let dir = tmp();
        let jdir = journal_dir(&dir);
        fs::create_dir_all(&jdir).unwrap();
        fs::write(jdir.join("seg-00000000000000000000.jsonl.tmp"), b"junk").unwrap();
        assert_eq!(detect_store_format(&dir), DetectedStoreFormat::Empty);
        let j = init(&dir).unwrap();
        drop(j);
        assert_eq!(
            fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
            STORE_VERSION
        );
    }

    #[test]
    fn unreadable_version_is_store_io() {
        let dir = tmp();
        fs::create_dir_all(dir.join("VERSION")).unwrap();
        assert!(matches!(
            detect_store_format(&dir),
            DetectedStoreFormat::Unreadable { .. }
        ));
        assert!(matches!(
            refuse_incompatible_store_format(&dir),
            Err(JournalHealth::StoreIo(_))
        ));
        let mut sink = NopSink;
        assert!(matches!(
            open(&dir, &mut sink),
            Err(OpenError::Health(JournalHealth::StoreIo(_)))
        ));
    }

    #[test]
    fn repair_stamps_migratable_store() {
        let dir = tmp();
        let j = init(&dir).unwrap();
        drop(j);
        let seg = journal_dir(&dir).join("seg-00000000000000000000.jsonl");
        let mut bytes = fs::read(&seg).unwrap();
        bytes.extend_from_slice(b"{\"partial");
        fs::write(&seg, bytes).unwrap();
        fs::write(dir.join("VERSION"), "5\n").unwrap();
        repair_truncate_torn_tail(&dir).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
            STORE_VERSION
        );
        let mut sink = NopSink;
        drop(open(&dir, &mut sink).unwrap());
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
