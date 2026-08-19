use std::path::PathBuf;

use fsm_core::json::Value;
use fsm_core::record::{Record, RecordKind, limits_value, seal, zeros};

use super::paths::{seg_name, sync_dir};
use super::types::{Journal, JournalIoError, Seg};
use super::{journal_dir, should_rotate};

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
