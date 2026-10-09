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

pub(super) fn run_admission_matrix() {
    assert_eq!(
        std::env::var("FSM_NATIVE_FIXTURE_DISPOSABLE").as_deref(),
        Ok("1")
    );
    assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 0);
    let seed = format!("{}-{:?}", std::process::id(), std::time::SystemTime::now());
    let nonce = fsm_core::sha256::to_hex(&fsm_core::sha256::sha256(seed.as_bytes()));
    let staging = PathBuf::from(format!("/usr/libexec/fsm-crash-{}", &nonce[..24]));
    fs::DirBuilder::new().mode(0o755).create(&staging).unwrap();
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o755)).unwrap();
    for (name, variable) in [
        ("contract-test", "CONTRACT"),
        ("fixture", "FIXTURE"),
        ("fsm", "CLI"),
    ] {
        super::super::workflow_cases::stage_artifact(
            &staging.join(name),
            &format!("FSM_CRASH_{variable}_ARTIFACT"),
            &format!("FSM_CRASH_{variable}_SHA256"),
        );
    }
    for behavior in [
        "contract-standalone",
        "contract-borrowed",
        "contract-fair-standalone",
        "contract-fair-borrowed",
        "contract-unknown-standalone",
        "contract-unknown-borrowed",
        "contract-argument-standalone",
        "contract-argument-borrowed",
        "contract-contention-standalone",
        "contract-contention-borrowed",
        "contract-manual-standalone",
        "contract-manual-borrowed",
        "contract-ack-only-standalone",
        "contract-ack-only-borrowed",
        "contract-recovery-standalone",
        "contract-recovery-borrowed",
        "contract-bound-standalone",
        "contract-bound-borrowed",
        "contract-cancel-standalone",
        "contract-cancel-borrowed",
    ] {
        for kind in ["process", "mcp"] {
            scenario(
                &staging,
                &nonce[..24],
                Scenario {
                    host: "contract",
                    kind,
                    behavior,
                },
            );
        }
    }
    fs::remove_dir_all(staging).unwrap();
}
