//! The manifest of a detached archive, and the walk that checks one.
//!
//! An archive is evidence only if someone without this tool can check it, so
//! the per-segment digests are **plain, undomained SHA-256 over the segment
//! file's exact bytes**. Every other hash in this workspace is
//! domain-separated; this one deliberately is not, because `sha256sum
//! seg-*.jsonl` has to reproduce it. An archive auditable only by the tool
//! that wrote it is a weaker artifact than one auditable by `coreutils`, and
//! the inconsistency buys exactly that.
//!
//! `archive_id` **is** domain-separated, under `fsm:archive:1`, and it is what
//! the `journal_sealed` record commits: the live chain names exactly one
//! archive as the origin of its detached prefix. It covers the manifest with
//! its own `archive_id` absent, since a hash cannot commit to itself.
//!
//! One seal, one archive, one manifest. A directory that already holds a
//! `MANIFEST` is refused rather than appended to or merged: merging two
//! archives is a feature with no correct semantics, and appending silently
//! produces a manifest describing bytes it did not hash.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use fsm_core::hashes::{ARCHIVE_DOMAIN, domain_hash};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::verify_line;
use fsm_core::sha256::{Sha256, to_hex};

use crate::store::ErrorObj;

/// On-disk archive manifest format tag.
pub const ARCHIVE_FORMAT: &str = "fsm.archive/1";

/// The file every archive directory holds exactly one of.
pub const MANIFEST_FILE: &str = "MANIFEST";

/// How much of a segment is read at a time while digesting it.
///
/// A sealed segment can be far larger than any single persistence unit — it is
/// a concatenation of many — so this is the one reader in the workspace that
/// streams rather than reading a whole unit into memory.
const DIGEST_CHUNK_BYTES: usize = 64 * 1024;

/// One archived segment, described so it can be checked without this tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivedSegment {
    pub name: String,
    pub first_seq: u64,
    pub last_seq: u64,
    /// Plain SHA-256 hex of the file's exact bytes. Not domain-separated, on
    /// purpose: `sha256sum` must reproduce it.
    pub sha256: String,
    pub bytes: u64,
}

/// Everything an archive says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub sealed_through_seq: u64,
    pub sealed_last_hash: String,
    pub first_seq: u64,
    /// The `prev` of the first archived record.
    ///
    /// Zero for the first archive a store ever takes, because its first record
    /// is genesis. For every later archive it is the previous archive's
    /// `sealed_last_hash`, which is what lets two archives of one store be
    /// chained together by whoever holds both — and what lets this archive's
    /// own chain walk start from a value it can check rather than from an
    /// origin it does not have.
    pub first_prev_hash: String,
    pub records: u64,
    pub segments: Vec<ArchivedSegment>,
}

fn unreadable(what: impl std::fmt::Display) -> ErrorObj {
    ErrorObj::new("io/read", format!("archive: {what}"))
}

fn corrupt(what: impl std::fmt::Display) -> ErrorObj {
    ErrorObj::new("store/chain_broken", format!("archive: {what}")).hint(
        "the archive's own bytes disagree with its manifest. Restore the archive from the copy \
         the seal was taken against; `fsm` writes an archive once and never rewrites one",
    )
}

impl Manifest {
    /// The manifest as a value, with `archive_id` present.
    pub fn to_value(&self) -> Value {
        let mut object = self.material();
        object.insert("archive_id".into(), Value::Str(self.archive_id()));
        Value::Obj(object)
    }

    /// The hashed material: every field except `archive_id`.
    fn material(&self) -> BTreeMap<String, Value> {
        let segments = self
            .segments
            .iter()
            .map(|segment| {
                Value::Obj(BTreeMap::from([
                    ("name".into(), Value::Str(segment.name.clone())),
                    (
                        "first_seq".into(),
                        Value::Num(segment.first_seq.to_string()),
                    ),
                    ("last_seq".into(), Value::Num(segment.last_seq.to_string())),
                    ("sha256".into(), Value::Str(segment.sha256.clone())),
                    ("bytes".into(), Value::Num(segment.bytes.to_string())),
                ]))
            })
            .collect();
        BTreeMap::from([
            ("format".into(), Value::Str(ARCHIVE_FORMAT.into())),
            (
                "sealed_through_seq".into(),
                Value::Num(self.sealed_through_seq.to_string()),
            ),
            (
                "sealed_last_hash".into(),
                Value::Str(self.sealed_last_hash.clone()),
            ),
            ("first_seq".into(), Value::Num(self.first_seq.to_string())),
            (
                "first_prev_hash".into(),
                Value::Str(self.first_prev_hash.clone()),
            ),
            ("records".into(), Value::Num(self.records.to_string())),
            ("segments".into(), Value::Arr(segments)),
        ])
    }

    /// The value the seal record commits, naming exactly this archive.
    pub fn archive_id(&self) -> String {
        format!(
            "sha256:{}",
            to_hex(&domain_hash(ARCHIVE_DOMAIN, &Value::Obj(self.material())))
        )
    }

    pub fn from_value(value: &Value) -> Result<Self, ErrorObj> {
        let object = value.as_obj().ok_or_else(|| unreadable("not an object"))?;
        if object.get("format").and_then(Value::as_str) != Some(ARCHIVE_FORMAT) {
            return Err(unreadable("format is not fsm.archive/1"));
        }
        let number = |key: &str| -> Result<u64, ErrorObj> {
            object
                .get(key)
                .and_then(Value::as_num)
                .and_then(|raw| raw.parse().ok())
                .ok_or_else(|| unreadable(format!("missing `{key}`")))
        };
        let text = |key: &str| -> Result<String, ErrorObj> {
            object
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| unreadable(format!("missing `{key}`")))
        };
        let mut segments = Vec::new();
        for entry in object
            .get("segments")
            .and_then(Value::as_arr)
            .ok_or_else(|| unreadable("missing `segments`"))?
        {
            let entry = entry
                .as_obj()
                .ok_or_else(|| unreadable("segment is not an object"))?;
            let field_number = |key: &str| -> Result<u64, ErrorObj> {
                entry
                    .get(key)
                    .and_then(Value::as_num)
                    .and_then(|raw| raw.parse().ok())
                    .ok_or_else(|| unreadable(format!("segment is missing `{key}`")))
            };
            let field_text = |key: &str| -> Result<String, ErrorObj> {
                entry
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .ok_or_else(|| unreadable(format!("segment is missing `{key}`")))
            };
            segments.push(ArchivedSegment {
                name: field_text("name")?,
                first_seq: field_number("first_seq")?,
                last_seq: field_number("last_seq")?,
                sha256: field_text("sha256")?,
                bytes: field_number("bytes")?,
            });
        }
        let manifest = Self {
            sealed_through_seq: number("sealed_through_seq")?,
            sealed_last_hash: text("sealed_last_hash")?,
            first_seq: number("first_seq")?,
            first_prev_hash: text("first_prev_hash")?,
            records: number("records")?,
            segments,
        };
        // Present only when the manifest was written by this tool; when it is
        // present it must be the one this content produces.
        if let Some(declared) = object.get("archive_id").and_then(Value::as_str)
            && declared != manifest.archive_id()
        {
            return Err(corrupt("archive_id does not match the manifest's contents"));
        }
        Ok(manifest)
    }
}

/// The plain SHA-256 of a file's exact bytes, streamed.
pub fn file_digest(path: &Path) -> Result<(String, u64), ErrorObj> {
    let mut file = crate::open_regular_file(path)
        .map_err(|error| unreadable(format!("open {}: {error}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; DIGEST_CHUNK_BYTES];
    let mut total = 0u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| unreadable(format!("read {}: {error}", path.display())))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total += read as u64;
    }
    Ok((to_hex(&hasher.finalize()), total))
}

pub fn manifest_path(archive_dir: &Path) -> PathBuf {
    archive_dir.join(MANIFEST_FILE)
}

/// Refuse an archive directory that already holds a manifest.
pub fn refuse_existing_manifest(archive_dir: &Path) -> Result<(), ErrorObj> {
    if manifest_path(archive_dir).exists() {
        return Err(ErrorObj::new(
            "store/archive_refused",
            format!("{} already holds a {MANIFEST_FILE}", archive_dir.display()),
        )
        .hint(
            "one seal, one archive, one manifest: name an empty directory with `--to`. Merging \
             two archives has no correct semantics, and appending to one produces a manifest that \
             describes bytes it did not hash",
        ));
    }
    Ok(())
}

pub fn read_manifest(archive_dir: &Path) -> Result<Manifest, ErrorObj> {
    let path = manifest_path(archive_dir);
    let bytes = crate::read_regular_file_capped(&path, crate::PERSISTENCE_READ_CAP)
        .map_err(|error| unreadable(format!("read {}: {error}", path.display())))?;
    let value = parse(&bytes, &JsonLimits::DEFAULT).map_err(|error| unreadable(error.message))?;
    Manifest::from_value(&value)
}

/// Check an archive against its manifest, bytes and chain.
///
/// Reports the **first** disagreement naming the segment responsible: a caller
/// who has to guess which segment failed will guess.
pub fn verify(archive_dir: &Path) -> Result<Manifest, ErrorObj> {
    let manifest = read_manifest(archive_dir)?;
    verify_digests(archive_dir, &manifest)?;
    verify_chain(archive_dir, &manifest)?;
    Ok(manifest)
}

fn verify_digests(archive_dir: &Path, manifest: &Manifest) -> Result<(), ErrorObj> {
    for segment in &manifest.segments {
        let path = archive_dir.join(&segment.name);
        if !path.exists() {
            return Err(corrupt(format!("segment {} is missing", segment.name)));
        }
        let (digest, bytes) = file_digest(&path)?;
        if digest != segment.sha256 {
            return Err(corrupt(format!(
                "segment {} hashes to {digest}, and the manifest says {}",
                segment.name, segment.sha256
            )));
        }
        if bytes != segment.bytes {
            return Err(corrupt(format!(
                "segment {} is {bytes} bytes, and the manifest says {}",
                segment.name, segment.bytes
            )));
        }
    }
    // An archive cannot be quietly extended: a segment file present but absent
    // from the manifest is bytes nobody hashed sitting beside bytes somebody
    // did, which is exactly what a reader would mistake for evidence.
    let declared: std::collections::BTreeSet<&str> = manifest
        .segments
        .iter()
        .map(|segment| segment.name.as_str())
        .collect();
    let entries = std::fs::read_dir(archive_dir)
        .map_err(|error| unreadable(format!("list {}: {error}", archive_dir.display())))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == MANIFEST_FILE || declared.contains(name.as_str()) {
            continue;
        }
        return Err(corrupt(format!(
            "{name} is in the archive directory and not in the manifest"
        )));
    }
    Ok(())
}

fn verify_chain(archive_dir: &Path, manifest: &Manifest) -> Result<(), ErrorObj> {
    let mut expect = manifest.first_seq;
    // The walk starts from the manifest's own declared predecessor rather than
    // from the chain origin: only a store's *first* archive begins at genesis.
    let mut previous = manifest.first_prev_hash.clone();
    let mut counted = 0u64;
    let mut last_hash = None;
    for segment in &manifest.segments {
        if segment.first_seq != expect {
            return Err(corrupt(format!(
                "segment {} starts at seq {} and the archive expected {expect}",
                segment.name, segment.first_seq
            )));
        }
        let path = archive_dir.join(&segment.name);
        let mut reader = crate::CappedLineReader::open(&path, crate::PERSISTENCE_READ_CAP)
            .map_err(|error| unreadable(format!("read {}: {error}", path.display())))?;
        while let Some(line) = reader
            .next_line()
            .map_err(|error| unreadable(format!("read {}: {error}", path.display())))?
        {
            if !line.terminated {
                return Err(corrupt(format!(
                    "segment {} ends in an unterminated record",
                    segment.name
                )));
            }
            if line.bytes.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let record = verify_line(&line.bytes, expect, &previous)
                .map_err(|error| corrupt(format!("segment {}: {error:?}", segment.name)))?;
            expect = record.seq + 1;
            previous = record.hash.clone();
            last_hash = Some(record.hash);
            counted += 1;
        }
        if expect != segment.last_seq + 1 {
            return Err(corrupt(format!(
                "segment {} ends at seq {} and the manifest says {}",
                segment.name,
                expect.saturating_sub(1),
                segment.last_seq
            )));
        }
    }
    if counted != manifest.records {
        return Err(corrupt(format!(
            "the archive holds {counted} records and the manifest says {}",
            manifest.records
        )));
    }
    if expect != manifest.sealed_through_seq + 1 {
        return Err(corrupt(format!(
            "the archive ends at seq {} and the manifest seals through {}",
            expect.saturating_sub(1),
            manifest.sealed_through_seq
        )));
    }
    if last_hash.as_deref() != Some(manifest.sealed_last_hash.as_str()) {
        return Err(corrupt(format!(
            "the record at seq {} hashes to {}, and the seal committed {}",
            manifest.sealed_through_seq,
            last_hash.unwrap_or_default(),
            manifest.sealed_last_hash
        )));
    }
    Ok(())
}
