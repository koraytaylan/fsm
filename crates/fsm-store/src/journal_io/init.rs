use std::fs;
use std::io::{ErrorKind, Write};
use std::path::Path;

use fsm_core::json::Value;
use fsm_core::record::{RecordKind, limits_value, seal, zeros};

use super::classify::classify;
use super::open::open;
use super::paths::{acquire_lock, io_err, seg_name, sync_dir};
use super::types::{Journal, JournalHealth, JournalIoError, OpenError};
use super::verify::refuse_incompatible_store_format;
use super::{DetectedStoreFormat, STORE_VERSION, detect_store_format, journal_dir};

pub(super) fn write_genesis_unlocked(jdir: &Path) -> Result<(), JournalIoError> {
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

pub(super) fn stamp_store_version(dir: &Path) -> Result<(), JournalIoError> {
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

pub(super) fn write_version_durable(dir: &Path) -> Result<(), JournalIoError> {
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
        .map(|(j, _, _, _)| j)
        .map_err(|e| match e {
            OpenError::ReadIo(s) | OpenError::WriteIo(s) => JournalIoError::Io(s),
            OpenError::Health(h) => JournalIoError::Io(h.message()),
        })
}
