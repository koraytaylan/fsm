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
    fs::File::open(staging).unwrap().sync_all().unwrap();
}
