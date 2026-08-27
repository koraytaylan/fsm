use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;
use fsm_core::record::{Record, RecordKind};
use fsm_core::replay::STATE_ROOT_FORMAT;

use crate::journal_io::JournalIoError;

use super::{ErrorObj, Store};

impl Store {
    /// Reject a value too large to journal.
    ///
    /// Checked before the request is applied, so an oversized payload costs
    /// nothing but the error — it never reaches the journal, which would carry
    /// it forever.
    pub(super) fn check_journalled_size(
        what: &str,
        v: &Value,
        request_id: &str,
    ) -> Result<(), ErrorObj> {
        let bytes = canon_bytes(v).len();
        if bytes <= fsm_core::limits::MAX_PAYLOAD_BYTES {
            return Ok(());
        }
        let max = fsm_core::limits::MAX_PAYLOAD_BYTES;
        let mut d = std::collections::BTreeMap::new();
        d.insert("field".into(), Value::Str(what.into()));
        d.insert("bytes".into(), Value::Num(bytes.to_string()));
        d.insert("max_bytes".into(), Value::Num(max.to_string()));
        Err(ErrorObj::new(
            "req/payload_too_large",
            format!("{what} is {bytes} bytes; the limit is {max}"),
        )
        .hint(format!(
            "journal records are permanent: send a digest or an identifier in {what} and keep the payload in your own store"
        ))
        .details(Value::Obj(d))
        .request_id(request_id))
    }

    pub(super) fn note_record(&mut self, rec: &Record) {
        if rec.kind == fsm_core::record::RecordKind::MachineDefined
            && let Some(machine_id) = rec
                .body
                .get("machine_id")
                .and_then(fsm_core::json::Value::as_str)
        {
            self.machine_seqs
                .entry(machine_id.into())
                .or_insert(rec.seq);
        }
        self.records.push(rec.clone());
        self.state.last_seq = rec.seq;
        self.state.last_hash = rec.hash.clone();
    }

    pub(super) fn finish_commit(&mut self) {
        self.pending_fp = None;
        self.after_commit();
    }

    /// Stamp the in-flight request's fingerprint onto any body that claims a
    /// `request_id`, so the fold can rebuild the idempotency ledger with the
    /// content each key was claimed for. Done in the one funnel every append
    /// passes through rather than at each body-building site.
    fn stamp_request_fp(&self, body: Value) -> Value {
        match (self.pending_fp.as_ref(), body) {
            (Some(fp), Value::Obj(mut o)) if o.contains_key("request_id") => {
                o.insert("request_fp".into(), Value::Str(fp.clone()));
                Value::Obj(o)
            }
            (_, other) => other,
        }
    }

    pub(super) fn journal_write_error(error: JournalIoError, request_id: Option<&str>) -> ErrorObj {
        let mut output = match error {
            JournalIoError::RecordTooLarge { bytes, max_bytes } => {
                let details = Value::Obj(std::collections::BTreeMap::from([
                    ("bytes".into(), Value::Num(bytes.to_string())),
                    ("max_bytes".into(), Value::Num(max_bytes.to_string())),
                ]));
                ErrorObj::new(
                    "io/write",
                    format!(
                        "journal record is {bytes} bytes; the limit is {max_bytes} bytes"
                    ),
                )
                .hint(
                    "shorten identifiers, creation overrides, tags, or cancellation reasons before retrying",
                )
                .details(details)
            }
            other => ErrorObj::new("io/write", other.to_string()),
        };
        if let Some(request_id) = request_id {
            output = output.request_id(request_id);
        }
        output
    }

    pub(super) fn append_at_with_root(
        &mut self,
        kind: RecordKind,
        body: Value,
        ts: i64,
    ) -> Result<Record, ErrorObj> {
        let body = self.stamp_request_fp(body);
        let request_id = body
            .get("request_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let seq = self.journal.last_seq.saturating_add(1);
        if !seq.is_multiple_of(10_000) {
            return self
                .journal
                .append_at(kind, body, ts)
                .map_err(|error| Self::journal_write_error(error, request_id.as_deref()));
        }
        let mut body = body.as_obj().cloned().ok_or_else(|| {
            ErrorObj::new("store/state_hash_mismatch", "record body is not an object")
        })?;
        let provisional = fsm_core::record::seal(
            seq,
            ts,
            kind,
            Value::Obj(body.clone()),
            &self.journal.last_hash,
        );
        let projected = fsm_core::replay::fold_from(
            self.state.clone(),
            [provisional],
            &mut fsm_core::replay::NopSink,
        )
        .map_err(|e| ErrorObj::new("store/state_hash_mismatch", format!("{e:?}")))?;
        body.insert(
            "state_root".into(),
            Value::Str(fsm_core::replay::state_root_at(&projected, seq)),
        );
        body.insert(
            "state_root_format".into(),
            Value::Str(STATE_ROOT_FORMAT.into()),
        );
        self.journal
            .append_at(kind, Value::Obj(body), ts)
            .map_err(|error| Self::journal_write_error(error, request_id.as_deref()))
    }

    pub(super) fn ensure_writable(&self) -> Result<(), ErrorObj> {
        if self.journal.is_read_only() {
            Err(ErrorObj::new("io/write", "store was opened read-only"))
        } else {
            Ok(())
        }
    }

    pub(super) fn append_rec(
        &mut self,
        kind: RecordKind,
        body: Value,
        clock: &mut dyn crate::clock::Clock,
    ) -> Result<Record, ErrorObj> {
        let ts = clock.now_ms();
        self.append_at_with_root(kind, body, ts)
    }

    fn after_commit(&mut self) {
        let _ = self.maybe_snapshot();
    }
}
