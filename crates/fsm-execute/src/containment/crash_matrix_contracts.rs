//! Root approval must carry the observer's exact native handler policy.

use super::*;

pub(super) fn sensitivity_case(behavior: &str) -> Option<&'static str> {
    match behavior {
        "contract-sensitivity-structure-standalone" => {
            Some("provisioned::sensitivity::standalone_structural_guard_refuses_real_handler_entry")
        }
        "contract-sensitivity-structure-borrowed" => {
            Some("provisioned::sensitivity::borrowed_structural_guard_refuses_real_handler_entry")
        }
        "contract-sensitivity-bound-standalone" => {
            Some("provisioned::sensitivity::standalone_bound_guard_refuses_real_handler_entry")
        }
        "contract-sensitivity-bound-borrowed" => {
            Some("provisioned::sensitivity::borrowed_bound_guard_refuses_real_handler_entry")
        }
        _ => None,
    }
}

pub(super) fn check_observer_verdict(
    behavior: &str,
    status: std::process::ExitStatus,
    output: &[u8],
) {
    let output = String::from_utf8_lossy(output);
    if status.success() {
        assert!(output.contains("1 passed; 0 failed; 0 ignored;"));
    } else if let Some(name) = sensitivity_case(behavior) {
        assert_eq!(
            status.code(),
            Some(101),
            "unexpected observer exit: {output}"
        );
        assert!(
            output.contains(&format!("test {name} ... FAILED")),
            "{output}"
        );
        assert!(
            output.contains("native contract guard permitted external entry"),
            "{output}"
        );
    } else {
        panic!("candidate matrix {behavior}: {output}");
    }
}

pub(super) fn sensitivity_claims(store: &Store, behavior: &str) -> Option<usize> {
    sensitivity_case(behavior)?;
    let claims = store
        .records
        .iter()
        .filter(|record| record.kind == fsm_core::record::RecordKind::ExecutionClaimed)
        .count();
    assert!(claims <= 1, "guard probe substituted another claim");
    if behavior.starts_with("contract-sensitivity-bound-") {
        assert_eq!(claims, 1, "bound probe must retain its original claim");
    }
    Some(claims)
}

pub(super) fn publish_sensitivity(
    fixture: &Fixture,
    resource: &Path,
    case: Scenario,
    passed: bool,
) {
    // Root independently checks the actual physical observation and entry permit;
    // verify() already authenticated closure and memory receipts for every launch.
    let entered = !fs::read(resource.join("root-entered")).unwrap().is_empty();
    let entries = fs::read_dir(&fixture.directory)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let name = name.to_str().unwrap();
            (name.starts_with("entry-") && name.ends_with(".json")).then_some(entry.path())
        })
        .count();
    assert_eq!(
        entries,
        usize::from(entered),
        "physical entry and protected permit disagree"
    );
    assert_eq!(
        passed, !entered,
        "observer verdict differs from Root entry proof"
    );
    writeln!(
        std::io::stdout().lock(),
        "FSM_NATIVE_CONTRACT_SENSITIVITY {} {} forbidden_entry={entered}",
        case.behavior,
        case.kind
    )
    .unwrap();
}

pub(super) fn run_sensitivity() {
    assert_eq!(
        std::env::var("FSM_NATIVE_FIXTURE_DISPOSABLE").as_deref(),
        Ok("1")
    );
    assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 0);
    let guard = std::env::var("FSM_CONTRACT_SENSITIVITY_GUARD").unwrap();
    assert!(matches!(guard.as_str(), "structure" | "bound"));
    let seed = format!("{}-{:?}", std::process::id(), std::time::SystemTime::now());
    let nonce = fsm_core::sha256::to_hex(&fsm_core::sha256::sha256(seed.as_bytes()));
    let staging = PathBuf::from(format!("/usr/libexec/fsm-crash-{}", &nonce[..24]));
    fs::DirBuilder::new().mode(0o755).create(&staging).unwrap();
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
    let behaviors = if guard == "structure" {
        [
            "contract-sensitivity-structure-standalone",
            "contract-sensitivity-structure-borrowed",
        ]
    } else {
        [
            "contract-sensitivity-bound-standalone",
            "contract-sensitivity-bound-borrowed",
        ]
    };
    let mut all_passed = true;
    for behavior in behaviors {
        for kind in ["process", "mcp"] {
            all_passed &= scenario(
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
    assert!(all_passed, "native contract guard permitted external entry");
}

#[test]
fn sensitivity_handlers_hold_real_entry_until_original_shutdown() {
    use fsm_execute::config::{HandlerKind, HandlerTable};
    let source = r#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","kind":"process","argv":["/fixture","process","/resource","hold-result"],"timeout_ms":30000,"retry":{"attempts":2,"backoff_ms":10,"max_backoff_ms":10,"on":["timeout"]},"on_ok":{"event":"done"}}]}"#;
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
        for behavior in [
            "contract-sensitivity-structure-standalone",
            "contract-sensitivity-structure-borrowed",
            "contract-sensitivity-bound-standalone",
            "contract-sensitivity-bound-borrowed",
        ] {
            assert!(sensitivity_case(behavior).is_some());
            let actual = super::table(
                Path::new("/fixture"),
                Path::new("/resource"),
                Scenario {
                    host: "contract",
                    kind,
                    behavior,
                },
            );
            let actual =
                HandlerTable::parse(&String::from_utf8(canon_bytes(&actual)).unwrap()).unwrap();
            assert_eq!(actual.handlers["notify"], expected);
        }
    }
    assert!(sensitivity_case("contract-standalone").is_none());
}

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
