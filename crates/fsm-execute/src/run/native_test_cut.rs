//! Opt-in disposable-CI journal barriers, absent from default host binaries.
//! A protected request binds the cut to the original physical store and run;
//! the external observer independently checks journal state and native closure.

use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    time::{Duration, Instant},
};

use fsm_core::{
    canon::canon_bytes,
    json::{JsonLimits, Value, parse},
    record::execution::Claim,
};
use fsm_store::store::Store;

use crate::error::ExecError;

pub(super) fn hold_journal_cut(store: &Store, claim: &Claim, cut: &str) -> Result<(), ExecError> {
    let Some(request) = std::env::var_os("FSM_LIFECYCLE_JOURNAL_CUT") else {
        return Ok(());
    };
    let request = read_protected_request(Path::new(&request))?;
    if request.get("cut").and_then(Value::as_str) != Some(cut) {
        return Ok(());
    }
    let physical = fs::metadata(&store.data_dir).map_err(failure)?;
    let material = claim.to_value();
    for (name, expected) in [
        ("device", Value::Num(physical.dev().to_string())),
        ("inode", Value::Num(physical.ino().to_string())),
        ("run_id", Value::Num(claim.run_id().to_string())),
        (
            "attempt",
            material
                .get("attempt")
                .ok_or_else(|| failure("claim attempt absent"))?
                .clone(),
        ),
    ] {
        if request.get(name) != Some(&expected) {
            return Err(failure("journal barrier does not match original store/run"));
        }
    }
    let ready = Value::Obj(BTreeMap::from([
        ("claim".into(), material),
        ("cut".into(), Value::Str(cut.into())),
        ("pid".into(), Value::Num(std::process::id().to_string())),
        ("seq".into(), Value::Num(store.journal.last_seq.to_string())),
    ]));
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(store.data_dir.join("journal-cut-ready.pending"))
        .map_err(failure)?;
    destination
        .write_all(&canon_bytes(&ready))
        .map_err(failure)?;
    destination.sync_all().map_err(failure)?;
    fs::rename(
        store.data_dir.join("journal-cut-ready.pending"),
        store.data_dir.join("journal-cut-ready.json"),
    )
    .map_err(failure)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    Err(failure(
        "disposable journal barrier expired without executor crash",
    ))
}

fn read_protected_request(path: &Path) -> Result<Value, ExecError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(failure("journal barrier request must be absolute"));
    }
    let mut directories = Vec::new();
    for ancestor in path.ancestors().skip(1) {
        let metadata = fs::symlink_metadata(ancestor).map_err(failure)?;
        if !metadata.is_dir() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            return Err(failure("journal barrier ancestor is not Root protected"));
        }
        directories.push((ancestor, metadata.dev(), metadata.ino()));
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(0o400000 | 0o4000)
        .open(path)
        .map_err(failure)?;
    let before = file.metadata().map_err(failure)?;
    if !before.is_file()
        || before.uid() != 0
        || before.mode() & 0o7777 != 0o444
        || before.nlink() != 1
        || before.len() > 65_536
    {
        return Err(failure(
            "journal barrier request is not bounded Root publication",
        ));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(65_537)
        .read_to_end(&mut bytes)
        .map_err(failure)?;
    if bytes.len() > 65_536 {
        return Err(failure("journal barrier request exceeds bound"));
    }
    let current = fs::symlink_metadata(path).map_err(failure)?;
    if (before.dev(), before.ino(), before.mode(), before.len())
        != (current.dev(), current.ino(), current.mode(), current.len())
    {
        return Err(failure("journal barrier request changed"));
    }
    for (ancestor, device, inode) in directories {
        let current = fs::symlink_metadata(ancestor).map_err(failure)?;
        if !current.is_dir()
            || current.uid() != 0
            || current.mode() & 0o022 != 0
            || (current.dev(), current.ino()) != (device, inode)
        {
            return Err(failure("journal barrier ancestor changed"));
        }
    }
    let value = parse(
        &bytes,
        &JsonLimits {
            max_bytes: 65_536,
            max_depth: 64,
        },
    )
    .map_err(|_| failure("journal barrier request JSON invalid"))?;
    if canon_bytes(&value) != bytes {
        return Err(failure("journal barrier request is not canonical"));
    }
    Ok(value)
}

fn failure(error: impl std::fmt::Display) -> ExecError {
    ExecError::new(
        "exec/inflight_deferred",
        format!("disposable journal cut: {error}"),
    )
}
