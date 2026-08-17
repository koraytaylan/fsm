#![no_main]
use libfuzzer_sys::fuzz_target;
use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;
use fsm_core::record::{verify_line, zeros};
use fsm_core::sha256::{sha256, to_hex};
use std::collections::BTreeMap;

fn independent_record_hash(
    seq: u64,
    ts: i64,
    kind: fsm_core::record::RecordKind,
    body: &Value,
    prev: &str,
) -> String {
    let mut m = BTreeMap::new();
    m.insert("seq".into(), Value::Num(seq.to_string()));
    m.insert("ts".into(), Value::Num(ts.to_string()));
    m.insert("kind".into(), Value::Str(kind.as_str().into()));
    m.insert("body".into(), body.clone());
    m.insert("prev".into(), Value::Str(prev.into()));
    let mut buf = b"fsm:record:1".to_vec();
    buf.push(0x0A);
    buf.extend_from_slice(&canon_bytes(&Value::Obj(m)));
    to_hex(&sha256(&buf))
}

fuzz_target!(|data: &[u8]| {
    if let Ok(rec) = verify_line(data, 0, &zeros()) {
        assert_eq!(rec.seq, 0);
        let recomputed =
            independent_record_hash(rec.seq, rec.ts, rec.kind, &rec.body, &rec.prev);
        assert_eq!(recomputed, rec.hash);
        let again = rec.to_line();
        assert_eq!(
            verify_line(&again, 0, &zeros()).map(|r| r.hash),
            Ok(rec.hash)
        );
    }
});
