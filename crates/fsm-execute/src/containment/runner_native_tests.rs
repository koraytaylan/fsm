//! Independent native MCP tree/pipe observer around production execution.

use super::super::super::{bind, identity, number, object, read_value, runner, text};
use super::{Fixture, claim_binding};
use fsm_core::json::Value;
use fsm_core::record::execution::NativeDomain;
use fsm_store::store::{Store, VerifiedClosure};
use std::fs;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SERVER: &str = r#"import json,os,subprocess,sys,time
from pathlib import Path
base=Path(sys.argv[1])
mode=sys.argv[2]
def publish(name,value):
    temporary=base/(name+'.pending')
    temporary.write_text(json.dumps(value,sort_keys=True,separators=(',',':')))
    temporary.replace(base/name)
def reply(request,result):
    print(json.dumps({'jsonrpc':'2.0','id':request['id'],'result':result}),flush=True)
child_code='''import json,os,subprocess,sys,time
from pathlib import Path
base=Path(sys.argv[1])
grandchild=subprocess.Popen(['/usr/bin/sleep','300'])
temporary=base/'descendant-ready.pending'
temporary.write_text(json.dumps({'pid':os.getpid(),'grandchild':grandchild.pid},sort_keys=True,separators=(',',':')))
temporary.replace(base/'descendant-ready')
time.sleep(300)
'''
for line in sys.stdin:
    request=json.loads(line)
    if request['method']=='initialize':
        reply(request,{})
    elif request['method']=='tools/call':
        assert request['params']['name']=='probe'
        assert request['params']['arguments']=={'fixture':'native'}
        child=subprocess.Popen(['/usr/bin/python3','-c',child_code,str(base)])
        deadline=time.monotonic()+2
        while not (base/'descendant-ready').exists():
            assert time.monotonic()<deadline
            time.sleep(.005)
        publish('root-ready',{'pid':os.getpid()})
        while not (base/'release').exists():
            assert time.monotonic()<deadline
            time.sleep(.005)
        sys.stderr.buffer.write(bytes(4097))
        sys.stderr.buffer.flush()
        if mode=='answer':
            reply(request,{'structuredContent':{'fixture':'native'},'isError':True})
        time.sleep(300)
"#;

struct Barriers {
    path: PathBuf,
    identity: Value,
}

impl Barriers {
    fn new() -> Self {
        let token = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // DynamicUser's strict system protection keeps /run read-only;
        // /dev/shm is a writable fixture resource, not an authority route.
        let path = PathBuf::from(format!(
            "/dev/shm/fsm-native-mcp-{}-{token}",
            std::process::id()
        ));
        fs::DirBuilder::new().mode(0o1777).create(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o1777)).unwrap();
        Self {
            identity: identity(&fs::symlink_metadata(&path).unwrap()),
            path,
        }
    }
}

impl Drop for Barriers {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|metadata| identity(&metadata) == self.identity)
        {
            // Delete only fixture-owned names, preserving unknown directory
            // entries and refusing to follow or delete a replaced namespace.
            for name in [
                "root-ready",
                "root-ready.pending",
                "descendant-ready",
                "descendant-ready.pending",
                "release",
            ] {
                let _ = fs::remove_file(self.path.join(name));
            }
            let _ = fs::remove_dir(&self.path);
        }
    }
}

pub(super) fn run() {
    for mode in ["answer", "timeout"] {
        let barriers = Barriers::new();
        let table = object([
            ("format", Value::Str("fsm.handlers/1".into())),
            (
                "handlers",
                Value::Arr(vec![object([
                    ("effect", Value::Str("notify".into())),
                    ("kind", Value::Str("mcp".into())),
                    (
                        "argv",
                        Value::Arr(
                            [
                                "/usr/bin/python3",
                                "-c",
                                SERVER,
                                barriers.path.to_str().unwrap(),
                                mode,
                            ]
                            .into_iter()
                            .map(|value| Value::Str(value.into()))
                            .collect(),
                        ),
                    ),
                    ("tool", Value::Str("probe".into())),
                    (
                        "arguments",
                        object([("fixture", Value::Str("native".into()))]),
                    ),
                    ("timeout_ms", Value::Num("3000".into())),
                    (
                        "retry",
                        object([
                            ("attempts", Value::Num("1".into())),
                            ("backoff_ms", Value::Num("10".into())),
                            ("max_backoff_ms", Value::Num("10".into())),
                            ("on", Value::Arr(vec![])),
                        ]),
                    ),
                ])]),
            ),
        ]);
        let mut fixture = Fixture::new_for_table(table);
        let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
        let (binding, effect) = claim_binding(&fixture, &domain);
        bind(&fixture.directory, &binding).unwrap();
        let directory = fixture.directory.clone();
        let execution = std::thread::spawn(move || runner::execute(&directory, 1));
        let deadline = Instant::now() + Duration::from_secs(3);
        while !barriers.path.join("root-ready").exists() {
            assert!(
                !execution.is_finished(),
                "native MCP execution ended before enrollment barrier"
            );
            assert!(
                Instant::now() < deadline,
                "native MCP enrollment barrier timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let root = read_value(&barriers.path.join("root-ready"), false).unwrap();
        let descendants = read_value(&barriers.path.join("descendant-ready"), false).unwrap();
        let pids = [
            number(&root, "pid").unwrap(),
            number(&descendants, "pid").unwrap(),
            number(&descendants, "grandchild").unwrap(),
        ];
        let unit = format!(
            "fsm-containment-{}-1-1.service",
            text(&domain.to_value(), "namespace").unwrap()
        );
        let membership = format!("0::/system.slice/{unit}\n");
        for pid in pids {
            assert_eq!(
                fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap(),
                membership
            );
            assert!((61184..=65519).contains(&fs::metadata(format!("/proc/{pid}")).unwrap().uid()));
        }
        let handoff = read_value(&fixture.directory.join("handoff-1.json"), true).unwrap();
        assert_eq!(
            number(handoff.get("gate").unwrap(), "pid").unwrap(),
            pids[0]
        );
        assert!(!execution.is_finished());
        assert!(
            fs::read_to_string(
                Path::new("/sys/fs/cgroup/system.slice")
                    .join(&unit)
                    .join("cgroup.events")
            )
            .unwrap()
            .lines()
            .any(|line| line == "populated 1")
        );
        fs::write(
            barriers.path.join("release"),
            b"independent enrollment verified",
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        while !execution.is_finished() {
            assert!(
                Instant::now() < deadline,
                "native MCP execution cleanup timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let result = execution.join().unwrap().unwrap();
        assert_eq!(result.get("claim"), binding.get("claim"));
        assert_eq!(result.get("journal_claim"), binding.get("journal_claim"));
        let candidate = result.get("candidate").unwrap();
        if mode == "answer" {
            assert_eq!(
                result.get("failure_class"),
                Some(&Value::Str("mcp_error".into()))
            );
            assert_eq!(
                candidate.get("error"),
                Some(&Value::Str("mcp/tool_error".into()))
            );
            assert_eq!(
                candidate.get("structured"),
                Some(&object([("fixture", Value::Str("native".into()))]))
            );
            assert_eq!(
                candidate
                    .get("stderr")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .as_bytes(),
                &[0; 4096]
            );
            assert_eq!(
                candidate.get("stderr_sha256"),
                Some(&Value::Str(
                    "b587fa297299ce9c602e58292b51379402bf7b1074f6b18679c2fb871c917ca8".into()
                ))
            );
        } else {
            assert_eq!(
                result.get("failure_class"),
                Some(&Value::Str("timeout".into()))
            );
            assert_eq!(
                candidate.get("error"),
                Some(&Value::Str("exec/timeout".into()))
            );
        }
        VerifiedClosure::read(Path::new(result.get("receipt").unwrap().as_str().unwrap())).unwrap();
        assert!(
            !Path::new("/sys/fs/cgroup/system.slice")
                .join(&unit)
                .exists()
        );
        let store = Store::open_read_only(&fixture.store).unwrap();
        assert!(
            store
                .state
                .execution
                .claim_for("instance", &effect)
                .is_some()
        );
        assert!(
            store
                .state
                .execution
                .stopped_for("instance", &effect)
                .is_none()
        );
        drop(store);
        assert!(runner::execute(&fixture.directory, 1).is_err());
        fixture.cleanup().unwrap();
    }
}
