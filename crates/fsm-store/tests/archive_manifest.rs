//! The `fsm.archive/1` manifest: its bytes, its digests, and the chain walk
//! that proves an archive is the prefix it claims to be.
//!
//! Plan 0017 task 8001. Regenerate the golden with `REGEN_ARCHIVE=1`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::{Record, RecordKind, limits_value, seal, zeros};
use fsm_core::sha256::{sha256, to_hex};
use fsm_store::archive::{
    ARCHIVE_FORMAT, ArchivedSegment, MANIFEST_FILE, Manifest, file_digest, manifest_path,
    read_manifest, refuse_existing_manifest, verify,
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// A directory name no other run of this binary can produce.
///
/// A process id alone is not unique enough: a full `--workspace` run spawns
/// thousands of short-lived processes, ids get reused, and a reused id names a
/// directory a previous run may still be finishing with — which surfaces as a
/// `store/lock` naming *this* process. `crash_harness.rs` learned the same
/// thing and pins it with a test; this is that idiom.
fn invocation_tag() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    )
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(tag: &str) -> Self {
        let index = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("fsm-archive-{tag}-{}-{index}", invocation_tag()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the temporary directory is creatable");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A chained run of records: genesis, then annotations under one instance.
fn chained_records(count: u64) -> Vec<Record> {
    let mut records = vec![seal(
        0,
        1,
        RecordKind::Genesis,
        Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.journal/1".into())),
            ("created_ts".into(), Value::Num("1".into())),
            ("limits".into(), limits_value()),
        ])),
        &zeros(),
    )];
    for seq in 1..count {
        let previous = records[records.len() - 1].hash.clone();
        records.push(seal(
            seq,
            seq as i64 + 1,
            RecordKind::StateCheckpoint,
            Value::Obj(BTreeMap::from([
                (
                    "state_root".into(),
                    Value::Str(fsm_core::replay::state_root_at(
                        &fsm_core::replay::StoreState::default(),
                        seq,
                    )),
                ),
                (
                    "state_root_format".into(),
                    Value::Str(fsm_core::replay::STATE_ROOT_FORMAT.into()),
                ),
            ])),
            &previous,
        ));
    }
    records
}

/// Write `records` into `archive` split across `segments` files, and return the
/// manifest that describes them.
fn build_archive(archive: &Path, records: &[Record], segments: usize) -> Manifest {
    let per_segment = records.len().div_ceil(segments);
    let mut described = Vec::new();
    for chunk in records.chunks(per_segment) {
        let name = format!("seg-{:020}.jsonl", chunk[0].seq);
        let path = archive.join(&name);
        let mut bytes = Vec::new();
        for record in chunk {
            bytes.extend_from_slice(&record.to_line());
        }
        fs::write(&path, &bytes).expect("the segment is writable");
        described.push(ArchivedSegment {
            name,
            first_seq: chunk[0].seq,
            last_seq: chunk[chunk.len() - 1].seq,
            sha256: to_hex(&sha256(&bytes)),
            bytes: bytes.len() as u64,
        });
    }
    let last = &records[records.len() - 1];
    Manifest {
        sealed_through_seq: last.seq,
        sealed_last_hash: last.hash.clone(),
        first_seq: records[0].seq,
        first_prev_hash: records[0].prev.clone(),
        records: records.len() as u64,
        segments: described,
    }
}

fn write_manifest(archive: &Path, manifest: &Manifest) {
    let mut bytes = canon_bytes(&manifest.to_value());
    bytes.push(b'\n');
    fs::write(manifest_path(archive), bytes).expect("the manifest is writable");
}

/// A well-formed archive of `count` records across `segments` files.
fn archive_of(directory: &TestDirectory, count: u64, segments: usize) -> Manifest {
    let records = chained_records(count);
    let manifest = build_archive(directory.path(), &records, segments);
    write_manifest(directory.path(), &manifest);
    manifest
}

#[test]
fn a_manifest_round_trips_and_the_golden_is_byte_exact() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/archive_manifest_v1.json");
    let directory = TestDirectory::create("golden");
    let manifest = archive_of(&directory, 6, 2);
    if std::env::var("REGEN_ARCHIVE").ok().as_deref() == Some("1") {
        let mut bytes = canon_bytes(&manifest.to_value());
        bytes.push(b'\n');
        fs::write(&fixture, bytes).expect("the golden is writable");
    }
    let committed = fs::read(&fixture).expect("the archive golden is committed");
    let expected = committed
        .strip_suffix(b"\n")
        .expect("text fixture ends in one LF");
    let parsed = parse(expected, &JsonLimits::DEFAULT).expect("the golden parses");
    assert_eq!(canon_bytes(&parsed), expected, "the fixture is canonical");
    assert_eq!(canon_bytes(&manifest.to_value()), expected);
    assert_eq!(
        parsed.get("format").and_then(Value::as_str),
        Some(ARCHIVE_FORMAT)
    );
    let restored = Manifest::from_value(&parsed).expect("the golden manifest decodes");
    assert_eq!(restored, manifest);
}

#[test]
fn a_well_formed_archive_verifies() {
    let directory = TestDirectory::create("ok");
    let manifest = archive_of(&directory, 12, 3);
    let verified = verify(directory.path()).expect("a well-formed archive verifies");
    assert_eq!(verified, manifest);
}

#[test]
fn one_flipped_byte_fails_and_names_the_segment_that_holds_it() {
    let directory = TestDirectory::create("flipped");
    let manifest = archive_of(&directory, 12, 3);
    let target = &manifest.segments[1];
    let path = directory.path().join(&target.name);
    let mut bytes = fs::read(&path).expect("the segment is readable");
    let position = bytes.len() / 2;
    bytes[position] ^= 0x20;
    fs::write(&path, &bytes).expect("the segment is writable");
    let error = verify(directory.path()).expect_err("a flipped byte is caught");
    assert!(
        error.message.contains(&target.name),
        "the error does not name the segment: {}",
        error.message
    );
}

#[test]
fn a_missing_segment_fails_and_names_it() {
    let directory = TestDirectory::create("missing");
    let manifest = archive_of(&directory, 12, 3);
    let target = &manifest.segments[2];
    fs::remove_file(directory.path().join(&target.name)).expect("the segment is removable");
    let error = verify(directory.path()).expect_err("a missing segment is caught");
    assert!(
        error.message.contains(&target.name) && error.message.contains("missing"),
        "the error does not name the missing segment: {}",
        error.message
    );
}

#[test]
fn an_extra_segment_the_manifest_does_not_name_fails() {
    // An archive cannot be quietly extended: bytes nobody hashed sitting beside
    // bytes somebody did are exactly what a reader mistakes for evidence.
    let directory = TestDirectory::create("extra");
    archive_of(&directory, 6, 2);
    fs::write(
        directory.path().join("seg-00000000000000009999.jsonl"),
        b"{}\n",
    )
    .expect("the extra file is writable");
    let error = verify(directory.path()).expect_err("an undeclared segment is caught");
    assert!(
        error.message.contains("9999"),
        "the error does not name the undeclared file: {}",
        error.message
    );
}

#[test]
fn a_sealed_last_hash_that_does_not_match_the_archived_record_fails() {
    let directory = TestDirectory::create("last-hash");
    let mut manifest = archive_of(&directory, 6, 2);
    manifest.sealed_last_hash = "ff".repeat(32);
    write_manifest(directory.path(), &manifest);
    let error = verify(directory.path()).expect_err("a foreign sealed_last_hash is caught");
    assert!(
        error.message.contains("hashes to"),
        "the error does not say which hash disagreed: {}",
        error.message
    );
}

#[test]
fn a_gap_in_the_sequence_range_fails() {
    let directory = TestDirectory::create("gap");
    let mut manifest = archive_of(&directory, 12, 3);
    // Claim the second segment starts one sequence later than it does.
    manifest.segments[1].first_seq += 1;
    write_manifest(directory.path(), &manifest);
    let error = verify(directory.path()).expect_err("a sequence gap is caught");
    assert!(
        error.message.contains("expected"),
        "the error does not describe the gap: {}",
        error.message
    );
}

#[test]
fn a_record_whose_predecessor_does_not_chain_fails() {
    let directory = TestDirectory::create("chain");
    let records = chained_records(6);
    // Rewrite the last record against the wrong predecessor: its own hash is
    // internally consistent, so only the chain walk can catch it.
    let mut broken = records.clone();
    let last = broken.len() - 1;
    broken[last] = seal(
        records[last].seq,
        records[last].ts,
        records[last].kind,
        records[last].body.clone(),
        &"ab".repeat(32),
    );
    let manifest = build_archive(directory.path(), &broken, 1);
    write_manifest(directory.path(), &manifest);
    let error = verify(directory.path()).expect_err("a broken chain is caught");
    assert_eq!(error.code, "store/chain_broken");
}

#[test]
fn an_archive_directory_that_already_holds_a_manifest_is_refused() {
    // One seal, one archive, one manifest.
    let directory = TestDirectory::create("occupied");
    assert!(refuse_existing_manifest(directory.path()).is_ok());
    archive_of(&directory, 6, 2);
    let error =
        refuse_existing_manifest(directory.path()).expect_err("an occupied directory is refused");
    assert_eq!(error.code, "store/archive_refused");
    assert!(
        error.message.contains(MANIFEST_FILE),
        "the refusal does not say what is in the way: {}",
        error.message
    );
}

#[test]
fn a_segment_digest_is_the_plain_digest_of_the_files_exact_bytes() {
    // Independently computed, so a domained or line-normalized digest fails:
    // `sha256sum seg-*.jsonl` has to reproduce what the manifest records.
    let directory = TestDirectory::create("plain");
    let manifest = archive_of(&directory, 6, 2);
    for segment in &manifest.segments {
        let path = directory.path().join(&segment.name);
        let bytes = fs::read(&path).expect("the segment is readable");
        assert_eq!(
            segment.sha256,
            to_hex(&sha256(&bytes)),
            "segment {} is not a plain digest of its own bytes",
            segment.name
        );
        assert_eq!(segment.bytes, bytes.len() as u64);
        let (streamed, size) = file_digest(&path).expect("the segment digests");
        assert_eq!(streamed, segment.sha256);
        assert_eq!(size, segment.bytes);
    }
}

#[test]
fn a_segment_larger_than_the_persistence_cap_digests_without_being_read_whole() {
    // A sealed segment is a concatenation of many persistence units, so this is
    // the one reader in the workspace that must stream.
    let directory = TestDirectory::create("oversized");
    let path = directory.path().join("large.bin");
    let chunk = vec![b'x'; 1024 * 1024];
    let mut expected = fsm_core::sha256::Sha256::new();
    {
        use std::io::Write;
        let mut file = fs::File::create(&path).expect("the large file is writable");
        for _ in 0..17 {
            file.write_all(&chunk).expect("the chunk is writable");
            expected.update(&chunk);
        }
    }
    let (digest, bytes) = file_digest(&path).expect("an oversized segment digests");
    assert_eq!(bytes, 17 * 1024 * 1024);
    assert!(
        bytes as usize > JsonLimits::DEFAULT.max_bytes,
        "the case must exceed the persistence read cap to prove anything"
    );
    assert_eq!(digest, to_hex(&expected.finalize()));
}

#[test]
fn the_archive_id_is_stable_and_moves_with_every_field() {
    let directory = TestDirectory::create("id");
    let manifest = archive_of(&directory, 6, 2);
    assert_eq!(manifest.archive_id(), manifest.archive_id());
    assert!(manifest.archive_id().starts_with("sha256:"));

    let baseline = manifest.archive_id();
    let mut moved = manifest.clone();
    moved.sealed_through_seq += 1;
    assert_ne!(moved.archive_id(), baseline);
    let mut moved = manifest.clone();
    moved.records += 1;
    assert_ne!(moved.archive_id(), baseline);
    let mut moved = manifest.clone();
    moved.first_seq += 1;
    assert_ne!(moved.archive_id(), baseline);
    let mut moved = manifest.clone();
    moved.sealed_last_hash = "cd".repeat(32);
    assert_ne!(moved.archive_id(), baseline);
    let mut moved = manifest.clone();
    moved.first_prev_hash = "12".repeat(32);
    assert_ne!(moved.archive_id(), baseline);
    let mut moved = manifest.clone();
    moved.segments[0].sha256 = "ef".repeat(32);
    assert_ne!(moved.archive_id(), baseline);
    let mut moved = manifest.clone();
    moved.segments[0].bytes += 1;
    assert_ne!(moved.archive_id(), baseline);
}

#[test]
fn a_manifest_whose_declared_archive_id_disagrees_with_its_contents_is_refused() {
    let directory = TestDirectory::create("forged");
    let manifest = archive_of(&directory, 6, 2);
    let mut object = manifest
        .to_value()
        .as_obj()
        .expect("the manifest is an object")
        .clone();
    object.insert(
        "archive_id".into(),
        Value::Str(format!("sha256:{}", "ab".repeat(32))),
    );
    let mut bytes = canon_bytes(&Value::Obj(object));
    bytes.push(b'\n');
    fs::write(manifest_path(directory.path()), bytes).expect("the manifest is writable");
    let error = read_manifest(directory.path()).expect_err("a forged archive_id is refused");
    assert_eq!(error.code, "store/chain_broken");
}

#[test]
fn a_later_archive_declares_the_predecessor_that_chains_it_to_the_earlier_one() {
    // A store's second archive does not begin at genesis. Its first record's
    // `prev` is the first archive's `sealed_last_hash`, and recording it is
    // what lets whoever holds both archives chain them — and what lets this
    // archive's own walk start from a value it can check.
    let directory = TestDirectory::create("later");
    for sub in ["a", "b"] {
        fs::create_dir_all(directory.path().join(sub)).expect("the sub-directory is creatable");
    }
    let whole = chained_records(9);
    let (earlier, later) = whole.split_at(4);
    let first = build_archive(&directory.path().join("a"), earlier, 1);
    let second = build_archive(&directory.path().join("b"), later, 1);
    assert_eq!(second.first_prev_hash, first.sealed_last_hash);
    write_manifest(&directory.path().join("b"), &second);
    verify(&directory.path().join("b")).expect("a later archive verifies on its own");
}

#[test]
fn a_manifest_declaring_another_format_is_refused() {
    let directory = TestDirectory::create("format");
    let manifest = archive_of(&directory, 6, 2);
    let mut object = manifest
        .to_value()
        .as_obj()
        .expect("the manifest is an object")
        .clone();
    object.remove("archive_id");
    object.insert("format".into(), Value::Str("fsm.archive/2".into()));
    fs::write(
        manifest_path(directory.path()),
        canon_bytes(&Value::Obj(object)),
    )
    .expect("the manifest is writable");
    let error = read_manifest(directory.path()).expect_err("a future format is refused");
    assert_eq!(error.code, "io/read");
}
