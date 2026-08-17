//! Kill-and-recover harness. 1,000 iterations; never lower the floor.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use fsm_cli::journal_io::{JournalHealth, classify, load_records, repair_truncate_torn_tail};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::replay::{NopSink, StoreState, fold_with};

fn child_mode() -> bool {
    std::env::var("FSM_CRASH_CHILD").is_ok()
}

fn case_def() -> Value {
    parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

const SCRIPT_LEN: usize = 16;

fn script_rids() -> Vec<String> {
    (0..SCRIPT_LEN).map(|i| format!("r{i}")).collect()
}

#[test]
fn crash_child() {
    let Ok(spec) = std::env::var("FSM_CRASH_CHILD") else {
        return;
    };
    let (dir, _seed) = spec.split_once(';').unwrap();
    let mut store = Store::open(PathBuf::from(dir).as_path()).unwrap();
    let _ = store.define_machine(case_def(), false, false);
    let blob = "x".repeat(64 * 1024);
    for i in 0..SCRIPT_LEN {
        let rid = format!("r{i}");
        if store
            .create_instance("case_review", &format!("i{i}"), &rid, None)
            .is_ok()
        {
            println!("{rid}");
        }
        if i == 0 || store.state.instances.contains_key("i0") {
            let _ = store.annotate("i0", &format!("n{i}"), &blob);
        }
    }
}

fn wait_classify(dir: &Path) -> JournalHealth {
    for _ in 0..200 {
        match classify(dir) {
            JournalHealth::LockIo(_) => {
                std::thread::sleep(Duration::from_millis(1));
            }
            other => return other,
        }
    }
    classify(dir)
}

fn recovered_rids(dir: &Path) -> Vec<String> {
    let recs = load_records(dir).unwrap_or_default();
    recs.into_iter()
        .filter_map(|r| {
            let id = r.body.get("request_id").and_then(Value::as_str)?;
            if id.starts_with('r') && id[1..].chars().all(|c| c.is_ascii_digit()) {
                Some(id.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn prefix_of(script: &[String], journaled: &[String]) -> bool {
    journaled.len() <= script.len() && script.iter().take(journaled.len()).eq(journaled.iter())
}

fn states_match(a: &StoreState, b: &StoreState) -> bool {
    a.machines.len() == b.machines.len()
        && a.instances.len() == b.instances.len()
        && a.instance_machines == b.instance_machines
        && a.instances.iter().all(|(id, st)| {
            b.instances
                .get(id)
                .map(|o| o.leaf == st.leaf && o.status == st.status)
                == Some(true)
        })
}

#[test]
fn crash_harness() {
    if child_mode() {
        return;
    }
    let want: u32 = std::env::var("FSM_CRASH_ITERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000)
        .max(1000);
    let mut seen_ok = false;
    let mut seen_torn = false;
    let mut seed = 0xF00D_u64;
    let script = script_rids();
    for it in 0..want {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let dir = std::env::temp_dir().join(format!("fsm-crash-{it}-{seed}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["crash_child", "--exact", "--nocapture"])
            .env("FSM_CRASH_CHILD", format!("{};{}", dir.display(), seed))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn");
        let delay = Duration::from_micros(100 + (seed % 25_000));
        std::thread::sleep(delay);
        let _ = child.kill();
        let mut stdout = Vec::new();
        if let Some(mut out) = child.stdout.take() {
            let _ = out.read_to_end(&mut stdout);
        }
        let _ = child.wait();
        let acked: Vec<String> = String::from_utf8_lossy(&stdout)
            .lines()
            .filter(|l| {
                let t = l.trim();
                t.starts_with('r') && t[1..].chars().all(|c| c.is_ascii_digit())
            })
            .map(|l| l.trim().to_string())
            .collect();

        let jdir = dir.join("journal");
        if !jdir.exists() {
            assert!(
                acked.is_empty(),
                "iter {it} seed {seed} acked {acked:?} but no journal"
            );
            seen_ok = true;
            let _ = fs::remove_dir_all(&dir);
            continue;
        }
        let health = wait_classify(&dir);
        match &health {
            JournalHealth::Ok | JournalHealth::MissingGenesis => seen_ok = true,
            JournalHealth::TornTail { .. } => {
                seen_torn = true;
                repair_truncate_torn_tail(&dir)
                    .unwrap_or_else(|e| panic!("iter {it} seed {seed} repair {e:?}"));
            }
            JournalHealth::LockIo(s) => {
                panic!("iter {it} seed {seed} lock lingered: {s}");
            }
            other => panic!("iter {it} seed {seed} unexpected {other:?}"),
        }

        let journaled = recovered_rids(&dir);
        assert!(
            prefix_of(&script, &journaled),
            "iter {it} seed {seed} journaled {journaled:?} is not a prefix of the script"
        );
        for id in &acked {
            assert!(
                journaled.contains(id),
                "iter {it} seed {seed} acked {id} missing from recovered journal {journaled:?}"
            );
        }

        let recs = load_records(&dir).unwrap_or_default();
        let folded = fold_with(recs, &mut NopSink)
            .unwrap_or_else(|e| panic!("iter {it} seed {seed} fold {e:?}"));
        if let Ok(live) = Store::open(&dir) {
            assert!(
                states_match(&live.state, &folded),
                "iter {it} seed {seed} live state != prefix fold"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }
    assert!(seen_ok, "no Ok classification across {want} kills");
    assert!(seen_torn, "no TornTail classification across {want} kills");
}
