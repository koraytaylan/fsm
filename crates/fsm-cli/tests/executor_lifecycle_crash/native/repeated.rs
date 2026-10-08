//! Repeated noisy trees in one independently pinned production host.

use super::*;
use fsm_core::canon::canon_bytes;
use fsm_core::record::execution::Claim;

const RUNS: usize = 12;

pub(super) fn observe(manifest: &Value, store: &Path, resource: &Path) {
    let mut document = parse(MACHINE, &JsonLimits::DEFAULT).unwrap();
    let Value::Obj(fields) = &mut document else {
        unreachable!()
    };
    fields.insert(
        "effects".into(),
        parse(
            br#"[{"name":"notify","fields":[{"name":"directory","ty":"str"}]}]"#,
            &JsonLimits::DEFAULT,
        )
        .unwrap(),
    );
    let mut states = Vec::new();
    let mut transitions = Vec::new();
    for index in 0..RUNS {
        let directory = resource.join(format!("run-{index}"));
        let expression =
            String::from_utf8(canon_bytes(&Value::Str(directory.to_str().unwrap().into())))
                .unwrap();
        states.push(object([
            ("name", Value::Str(format!("running_{index}"))),
            (
                "entry",
                object([(
                    "emit",
                    Value::Arr(vec![object([
                        ("effect", Value::Str("notify".into())),
                        ("args", object([("directory", Value::Str(expression))])),
                    ])]),
                )]),
            ),
        ]));
        transitions.push(object([
            ("from", Value::Str(format!("running_{index}"))),
            ("on", Value::Str("done".into())),
            (
                "to",
                Value::Str(if index + 1 == RUNS {
                    "finished".into()
                } else {
                    format!("running_{}", index + 1)
                }),
            ),
        ]));
    }
    states.push(object([
        ("name", Value::Str("finished".into())),
        ("terminal", Value::Bool(true)),
    ]));
    fields.insert("states".into(), Value::Arr(states));
    fields.insert("transitions".into(), Value::Arr(transitions));
    fields.insert("initial".into(), Value::Str("running_0".into()));
    let mut writer = Store::open(store).unwrap();
    writer.define_machine(document, false, false).unwrap();
    writer
        .create_instance("lifecycle_crash", "instance", "create", None)
        .unwrap();
    drop(writer);
    let mut host = Host::start(manifest, "original");
    let host_identity = identity(host.process.id());
    let mut previous_members = Vec::new();
    let mut baseline = None;
    for index in 0..RUNS {
        let directory = resource.join(format!("run-{index}"));
        let deadline = Instant::now() + Duration::from_secs(8);
        while !directory.join("root-candidate").is_file() {
            assert!(
                host.process.try_wait().unwrap().is_none(),
                "host exited: {}",
                host.diagnostics()
            );
            assert!(
                Instant::now() < deadline,
                "run {index} failed to enter: {}",
                host.diagnostics()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(identity(host.process.id()), host_identity);
        assert!(
            previous_members.iter().all(|member| !live(member)),
            "prior tree overlaps run {index}"
        );
        let snapshot = Store::open_read_only(store).unwrap();
        let claims: Vec<_> = snapshot
            .records
            .iter()
            .filter(|record| record.kind == RecordKind::ExecutionClaimed)
            .collect();
        assert_eq!(claims.len(), index + 1);
        for record in claims.iter().take(index) {
            let claim = claim(record);
            let proof =
                VerifiedClosure::read(&Path::new(field(manifest, "authority")).join(format!(
                    "closure-{}-{}.json",
                    claim
                        .domain()
                        .to_value()
                        .get("allocation")
                        .unwrap()
                        .as_num()
                        .unwrap(),
                    claim.run_id()
                )))
                .unwrap();
            assert!(proof.matches_claim(&claim, &format!("sha256:{}", record.hash)));
            proof.check_store(store).unwrap();
        }
        for kind in [
            RecordKind::ExecutionStopped,
            RecordKind::ExecutionSettled,
            RecordKind::EventApplied,
        ] {
            assert_eq!(
                snapshot
                    .records
                    .iter()
                    .filter(|record| record.kind == kind)
                    .count(),
                index
            );
        }
        let current_domain = claim(claims.last().unwrap()).domain().to_value();
        drop(snapshot);
        previous_members = ["root", "child", "grandchild"]
            .into_iter()
            .map(|role| {
                identity(
                    fs::read_to_string(directory.join(format!("{role}-entered")))
                        .unwrap()
                        .parse()
                        .unwrap(),
                )
            })
            .collect();
        assert!(previous_members.iter().all(live));
        for member in &previous_members {
            assert_eq!(
                fs::read_to_string(format!("/proc/{}/cgroup", member.0)).unwrap(),
                format!(
                    "0::/system.slice/fsm-containment-{}-{}-{}.service\n",
                    field(&current_domain, "namespace"),
                    current_domain.get("generation").unwrap().as_num().unwrap(),
                    current_domain.get("allocation").unwrap().as_num().unwrap(),
                )
            );
        }
        let observed = resources(host.process.id());
        writeln!(
            std::io::stdout().lock(),
            "FSM_NATIVE_RESOURCE_OBSERVATION {}",
            String::from_utf8(canon_bytes(&object([
                ("host", Value::Str(field(manifest, "host").into())),
                ("kind", Value::Str(field(manifest, "kind").into())),
                ("run", Value::Num(index.to_string())),
                ("descriptors", Value::Num(observed.0.to_string())),
                ("threads", Value::Num(observed.1.to_string())),
                ("rss_kib", Value::Num(observed.2.to_string())),
            ])))
            .unwrap()
        )
        .unwrap();
        if index == 1 {
            baseline = Some(observed);
        }
        if let Some((descriptors, threads, memory)) = baseline {
            // Compare the same held-candidate phase after one warm-up run;
            // allow two transient bookkeeping handles/tasks, not one per run.
            assert!(
                observed.0 <= descriptors + 2,
                "host descriptors accumulate: {observed:?}"
            );
            assert!(
                observed.1 <= threads + 2,
                "host threads accumulate: {observed:?}"
            );
            assert!(
                observed.2 <= memory + 16 * 1024,
                "host RSS grows beyond 16 MiB: {observed:?}"
            );
        }
        fs::write(directory.join("root-release"), b"release only root").unwrap();
    }
    wait_for_completion(store, &host);
    assert!(previous_members.iter().all(|member| !live(member)));
    assert_eq!(identity(host.process.id()), host_identity);
    let snapshot = Store::open_read_only(store).unwrap();
    for kind in [
        RecordKind::ExecutionClaimed,
        RecordKind::ExecutionStopped,
        RecordKind::ExecutionSettled,
        RecordKind::EventApplied,
    ] {
        assert_eq!(
            snapshot
                .records
                .iter()
                .filter(|record| record.kind == kind)
                .count(),
            RUNS
        );
    }
}

fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Obj(
        fields
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
    )
}

fn claim(record: &fsm_core::record::Record) -> Claim {
    let mut fields = record.body.as_obj().unwrap().clone();
    fields.remove("request_id");
    fields.remove("request_fp");
    Claim::from_value(&Value::Obj(fields)).unwrap()
}

fn resources(pid: u32) -> (usize, usize, usize) {
    let count = |name| {
        fs::read_dir(format!("/proc/{pid}/{name}"))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .len()
    };
    let status = fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    let memory = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    (count("fd"), count("task"), memory)
}
