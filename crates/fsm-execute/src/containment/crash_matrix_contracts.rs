//! Root approval must carry the observer's exact native handler policy.

use super::*;

pub(super) fn apply_policy(handler: &mut BTreeMap<String, Value>, behavior: &str) {
    if matches!(
        behavior,
        "contract-contention-standalone" | "contract-contention-borrowed"
    ) {
        handler.insert("timeout_ms".into(), Value::Num("5000".into()));
        handler.insert("on_failed".into(), handler["on_ok"].clone());
    }
    if matches!(
        behavior,
        "contract-ack-only-standalone" | "contract-ack-only-borrowed"
    ) {
        handler.remove("on_ok");
        handler.remove("on_failed");
        let Value::Obj(retry) = handler.get_mut("retry").unwrap() else {
            unreachable!()
        };
        retry.insert("attempts".into(), Value::Num("1".into()));
    }
}

#[test]
fn protected_contract_policies_match_independent_handler_contracts() {
    use fsm_execute::config::{HandlerKind, HandlerTable};
    let timeout = r#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","kind":"process","argv":["/fixture","process","/resource","hold-result"],"timeout_ms":5000,"retry":{"attempts":2,"backoff_ms":10,"max_backoff_ms":10,"on":["timeout"]},"on_ok":{"event":"done"},"on_failed":{"event":"done"}}]}"#;
    let ack_only = r#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","kind":"process","argv":["/fixture","process","/resource","hold-result"],"timeout_ms":30000,"retry":{"attempts":1,"backoff_ms":10,"max_backoff_ms":10,"on":["timeout"]}}]}"#;
    for (source, behaviors) in [
        (
            timeout,
            [
                "contract-contention-standalone",
                "contract-contention-borrowed",
            ],
        ),
        (
            ack_only,
            ["contract-ack-only-standalone", "contract-ack-only-borrowed"],
        ),
    ] {
        for kind in ["process", "mcp"] {
            let mut expected = HandlerTable::parse(source)
                .unwrap()
                .handlers
                .remove("notify")
                .unwrap();
            if kind == "mcp" {
                expected.argv[1] = "mcp".into();
                expected.kind = HandlerKind::Mcp {
                    tool: "run".into(),
                    arguments: Value::Obj(BTreeMap::new()),
                };
            }
            for behavior in behaviors {
                let actual = super::table(
                    Path::new("/fixture"),
                    Path::new("/resource"),
                    Scenario {
                        host: "contract",
                        kind,
                        behavior,
                    },
                );
                let table =
                    HandlerTable::parse(&String::from_utf8(canon_bytes(&actual)).unwrap()).unwrap();
                assert_eq!(table.handlers["notify"], expected);
                assert_eq!(
                    table.handlers["notify"].fingerprint(),
                    expected.fingerprint()
                );
            }
        }
    }
}

pub(super) fn prepare_observations(resource: &Path, behavior: &str) {
    if matches!(
        behavior,
        "contract-contention-standalone" | "contract-contention-borrowed"
    ) {
        // DynamicUser RemoveIPC must not unlink the original result observation
        // when timeout closes the first domain, before retry checks can read it.
        let candidate = resource.join("root-candidate");
        fs::write(&candidate, b"").unwrap();
        fs::set_permissions(candidate, fs::Permissions::from_mode(0o666)).unwrap();
    }
}
