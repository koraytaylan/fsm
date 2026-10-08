//! Production inspection remains read-only even beside the original writer.

use fsm_store::store::Store;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn files(directory: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(files(&path));
        } else {
            result.insert(path.clone(), fs::read(path).unwrap());
        }
    }
    result
}

fn directory(name: &str) -> PathBuf {
    let cache = PathBuf::from(std::env::var_os("TMPDIR").expect("explicit task cache"));
    assert!(!cache.starts_with("/tmp"));
    cache.join(format!("inspect-{name}-{}", std::process::id()))
}

#[test]
fn production_runs_inspection_preserves_all_bytes_while_writer_is_held() {
    let directory = directory("held");
    let writer = Store::open(&directory).unwrap();
    let before = files(&directory);
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&directory)
        .args(["execute", "runs"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("fsm.execution-runs/1"));
    assert!(text.contains("\"runs\":[]"));
    assert_eq!(files(&directory), before);
    drop(writer);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn production_runs_inspection_does_not_initialize_missing_store() {
    let directory = directory("missing");
    assert!(!directory.exists());
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&directory)
        .args(["execute", "runs"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!directory.exists());
}
