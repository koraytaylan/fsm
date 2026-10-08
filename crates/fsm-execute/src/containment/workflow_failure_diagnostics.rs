//! Preserve bounded original owner diagnostics before failed fixture teardown.

use super::*;
use std::io::{Seek, SeekFrom};

pub(super) fn archive(fixture: &Fixture, staging: &Path) {
    let namespace = fixture
        .directory
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    for name in [
        "stderr",
        "race-stderr",
        "race-stdout",
        "first-stderr",
        "first-stdout",
        "after-kill-stderr",
        "after-kill-stdout",
        "original-stderr",
        "immediate-restart-stderr",
        "verified-restart-stderr",
    ] {
        let path = fixture.store.join(name);
        let mut source = match fs::OpenOptions::new()
            .read(true)
            .custom_flags(super::super::super::super::NOFOLLOW_NONBLOCK)
            .open(&path)
        {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => panic!("original workflow diagnostic {}: {error}", path.display()),
        };
        let metadata = source.metadata().unwrap();
        assert!(metadata.is_file());
        // Logs can grow while an owner contends; preserve the last 64 KiB
        // without reading their unbounded prefix into memory.
        source
            .seek(SeekFrom::Start(metadata.len().saturating_sub(65536)))
            .unwrap();
        let mut destination = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(staging.join(format!("failure-{namespace}-{name}.log")))
            .unwrap();
        std::io::copy(&mut source.take(65536), &mut destination).unwrap();
        destination.sync_all().unwrap();
    }
    let snapshot = native_snapshot(fixture);
    let mut destination = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(staging.join(format!("failure-{namespace}-native.json")))
        .unwrap();
    let encoded = canon_bytes(&snapshot);
    assert!(encoded.len() <= 65536);
    destination.write_all(&encoded).unwrap();
    destination.sync_all().unwrap();
    fs::File::open(staging).unwrap().sync_all().unwrap();
}

fn native_snapshot(fixture: &Fixture) -> Value {
    let mut files = Vec::new();
    let mut truncated = false;
    for entry in fs::read_dir(&fixture.directory).unwrap() {
        if files.len() == 64 {
            truncated = true;
            break;
        }
        let entry = entry.unwrap();
        let metadata = fs::symlink_metadata(entry.path()).unwrap();
        files.push(object([
            (
                "name",
                Value::Str(entry.file_name().to_string_lossy().into_owned()),
            ),
            ("identity", super::super::super::identity(&metadata)),
            ("size", Value::Num(metadata.len().to_string())),
        ]));
    }
    let mut claims = Vec::new();
    let mut store_error = Value::Null;
    match Store::open_read_only(&fixture.store) {
        Ok(store) => {
            for (claim, stopped) in store.state.execution.unresolved().take(4) {
                let domain = claim.domain().to_value();
                let unit = format!(
                    "fsm-containment-{}-{}-{}.service",
                    text(&domain, "namespace").unwrap(),
                    number(&domain, "generation").unwrap(),
                    number(&domain, "allocation").unwrap(),
                );
                let cgroup = Path::new("/sys/fs/cgroup/system.slice").join(unit);
                let observations = [
                    "cgroup.events",
                    "cgroup.procs",
                    "memory.events",
                    "pids.current",
                ]
                .into_iter()
                .map(|name| (name.to_owned(), bounded_observation(&cgroup.join(name))))
                .collect();
                claims.push(object([
                    ("claim", claim.to_value()),
                    ("stopped", Value::Bool(stopped.is_some())),
                    ("kernel_observations", Value::Obj(observations)),
                ]));
            }
            truncated |= store.state.execution.unresolved().count() > 4;
        }
        Err(error) => store_error = Value::Str(error.message.chars().take(512).collect()),
    }
    object([
        (
            "scope",
            Value::Str("failed-native-workflow-observation".into()),
        ),
        ("closure_proved", Value::Bool(false)),
        ("authority_files", Value::Arr(files)),
        ("unresolved_claims", Value::Arr(claims)),
        ("store_error", store_error),
        ("truncated", Value::Bool(truncated)),
    ])
}

fn bounded_observation(path: &Path) -> Value {
    let result = (|| -> std::io::Result<String> {
        let mut source = fs::OpenOptions::new()
            .read(true)
            .custom_flags(super::super::super::super::NOFOLLOW_NONBLOCK)
            .open(path)?;
        let mut bytes = Vec::new();
        (&mut source).take(513).read_to_end(&mut bytes)?;
        if bytes.len() > 512 {
            return Err(std::io::Error::other("kernel observation exceeds bound"));
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    })();
    match result {
        Ok(text) => object([("observed", Value::Str(text))]),
        Err(error) => object([("error", Value::Str(error.to_string()))]),
    }
}
