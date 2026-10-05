//! Real claimed process/MCP capture boundaries and repeated owned retirement.

use super::super::super::{bind, object, runner};
use super::{Fixture, approved_table, claim_binding};
use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;
use fsm_core::record::execution::{Claim, NativeDomain};
use fsm_core::sha256::{sha256, to_hex};
use fsm_execute::run::{ACK_OUTPUT_CAP, MAX_CAPTURE_READ_BYTES};
use fsm_store::store::{Store, VerifiedClosure};
use std::fs;
use std::path::Path;

const SCRIPT: &str = r#"import json,sys,time
size=int(sys.argv[1])
def emit(stream):
    remaining=size
    chunk=b'x'*8192
    while remaining:
        count=min(remaining,len(chunk))
        stream.write(chunk[:count])
        remaining-=count
    stream.flush()
if sys.argv[2]=='process':
    emit(sys.stdout.buffer)
    emit(sys.stderr.buffer)
else:
    for line in sys.stdin:
        request=json.loads(line)
        if request['method']=='initialize':
            print(json.dumps(dict(jsonrpc='2.0',id=request['id'],result=dict())),flush=True)
        elif request['method']=='tools/call':
            emit(sys.stderr.buffer)
            print(json.dumps(dict(jsonrpc='2.0',id=request['id'],result=dict(structuredContent=dict(fixture='capture'),isError=True))),flush=True)
            time.sleep(300)
"#;

pub(super) fn run() {
    for mcp in [false, true] {
        for size in [
            ACK_OUTPUT_CAP,
            ACK_OUTPUT_CAP + 1,
            MAX_CAPTURE_READ_BYTES,
            MAX_CAPTURE_READ_BYTES + 1,
            MAX_CAPTURE_READ_BYTES * 8,
        ] {
            let before = resources();
            run_case(size, mcp);
            assert_eq!(resources(), before, "native capture leaked owned resources");
        }
    }
}

fn resources() -> (usize, usize) {
    fn count(path: &str) -> usize {
        fs::read_dir(path)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .len()
    }
    (count("/proc/self/fd"), count("/proc/self/task"))
}

fn run_case(size: usize, mcp: bool) {
    let Value::Obj(mut document) = approved_table() else {
        unreachable!()
    };
    let Value::Arr(handlers) = document.get_mut("handlers").unwrap() else {
        unreachable!()
    };
    let Value::Obj(handler) = &mut handlers[0] else {
        unreachable!()
    };
    handler.insert("timeout_ms".into(), Value::Num("10000".into()));
    handler.insert(
        "argv".into(),
        Value::Arr(
            [
                "/usr/bin/python3".into(),
                "-c".into(),
                SCRIPT.into(),
                size.to_string(),
                if mcp { "mcp" } else { "process" }.into(),
            ]
            .into_iter()
            .map(Value::Str)
            .collect(),
        ),
    );
    if mcp {
        handler.insert("kind".into(), Value::Str("mcp".into()));
        handler.insert("tool".into(), Value::Str("probe".into()));
        handler.insert("arguments".into(), object([]));
    }
    let mut fixture = Fixture::new_for_table(Value::Obj(document));
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let (binding, effect) = claim_binding(&fixture, &domain);
    bind(&fixture.directory, &binding).unwrap();
    let result = runner::execute(&fixture.directory, 1).unwrap();
    assert!(canon_bytes(&result).len() <= 65536);
    assert_eq!(result.get("failure_class"), Some(&Value::Null));
    let candidate = result.get("candidate").unwrap();
    if mcp {
        assert_eq!(
            candidate.get("error"),
            Some(&Value::Str("mcp/tool_error".into()))
        );
        assert_eq!(
            candidate.get("structured"),
            Some(&object([("fixture", Value::Str("capture".into()))]))
        );
    } else {
        assert_eq!(candidate.get("status"), Some(&Value::Num("0".into())));
        capture(candidate, "stdout", size);
    }
    capture(candidate, "stderr", size);
    let claim = Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let receipt =
        VerifiedClosure::read(Path::new(result.get("receipt").unwrap().as_str().unwrap())).unwrap();
    assert!(receipt.matches_claim(
        &claim,
        binding.get("journal_claim").unwrap().as_str().unwrap()
    ));
    assert!(!fixture.groups[0].0.exists());
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(
        store.state.execution.claim_for("instance", &effect),
        Some(&claim)
    );
    assert!(
        store
            .state
            .execution
            .stopped_for("instance", &effect)
            .is_none()
    );
    drop(store);
    fixture.cleanup().unwrap();
}

fn capture(candidate: &Value, stream: &str, size: usize) {
    assert_eq!(
        candidate.get(stream),
        Some(&Value::Str("x".repeat(size.min(ACK_OUTPUT_CAP))))
    );
    let expected = (size > ACK_OUTPUT_CAP && size <= MAX_CAPTURE_READ_BYTES)
        .then(|| Value::Str(to_hex(&sha256(&vec![b'x'; size]))));
    assert_eq!(
        candidate.get(&format!("{stream}_sha256")),
        expected.as_ref()
    );
}
