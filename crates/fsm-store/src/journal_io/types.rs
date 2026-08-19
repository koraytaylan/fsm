use std::fs::File;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;

use fsm_core::record::{Record, RecordError};

pub(super) enum Seg {
    File(File),
    Memory(Vec<u8>),
    ReadOnly,
}

impl Seg {
    pub(super) fn write_line(&mut self, line: &[u8]) -> std::io::Result<()> {
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
    pub(super) seg: Seg,
    pub seg_name: String,
    pub seg_first_seq: u64,
    pub seg_bytes: u64,
    pub seg_records: u32,
    pub last_seq: u64,
    pub last_hash: String,
    pub poisoned: bool,
    pub(super) _lock: Option<File>,
    pub(super) mem_records: Option<Vec<Record>>,
}

/// Failures returned by direct journal creation and append operations.
///
/// `RecordTooLarge` is part of the current persistence-boundary contract. Code
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
