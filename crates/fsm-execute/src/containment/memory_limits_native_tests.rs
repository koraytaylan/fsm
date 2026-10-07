//! Namespace-specific test service limits, retained with any failed fixture.
use super::*;
use std::os::unix::fs::OpenOptionsExt;
use std::process::Command;

const CHECK: &[u8] = br#"import json,os,sys
from pathlib import Path
a=Path(sys.argv[1])
group=next(line[3:] for line in Path('/proc/self/cgroup').read_text().splitlines() if line.startswith('0::'))
p=Path('/sys/fs/cgroup')/group.lstrip('/')
allocation=int(p.name.removesuffix('.service').rsplit('-',1)[1])
domain=json.loads((a/f'prepared-{allocation}.json').read_text())['domain']
m=p.stat()
assert p.name==f"fsm-containment-{domain['namespace']}-{domain['generation']}-{allocation}.service"
assert (m.st_dev,m.st_ino)==(domain['cgroup']['device'],domain['cgroup']['inode'])
memory=(p/'memory.max').read_text().strip();swap=(p/'memory.swap.max').read_text().strip()
assert memory=='1073741824' and swap=='0'
receipt=dict(domain=domain,memory_max=memory,memory_swap_max=swap)
fd=os.open(a/f'fixture-memory-{allocation}.json',os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
with os.fdopen(fd,'w') as f:json.dump(receipt,f,sort_keys=True,separators=(',',':'));f.flush();os.fsync(f.fileno())
"#;

pub(super) struct Limits {
    directory: PathBuf,
    directory_identity: Value,
    file_identity: Value,
    configuration: Vec<u8>,
}

impl Limits {
    pub(super) fn install(fixture: &Fixture) -> Self {
        let namespace = fixture.directory.parent().unwrap().file_name().unwrap();
        let directory = Path::new("/run/systemd/system").join(format!(
            "fsm-containment-{}-.service.d",
            namespace.to_str().unwrap()
        ));
        protected_directory(directory.parent().unwrap()).unwrap();
        fs::DirBuilder::new()
            .mode(0o755)
            .create(&directory)
            .unwrap();
        let checker = fixture.directory.join("fixture-memory-check.py");
        let mut checker_file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o500)
            .open(&checker)
            .unwrap();
        checker_file.write_all(CHECK).unwrap();
        checker_file.sync_all().unwrap();
        // The test preflight alone runs privileged; the original gate and
        // handler retain their DynamicUser and cgroup access restrictions.
        let configuration = format!(
            "[Service]\nMemoryMax=1G\nMemorySwapMax=0\nExecStartPre=+/usr/bin/python3 {} {}\n",
            checker.display(),
            fixture.directory.display()
        )
        .into_bytes();
        let path = directory.join("90-fsm-workflow-memory.conf");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&path)
            .unwrap();
        file.write_all(&configuration).unwrap();
        file.sync_all().unwrap();
        let limits = Self {
            directory_identity: identity(&fs::symlink_metadata(&directory).unwrap()),
            file_identity: identity(&file.metadata().unwrap()),
            directory,
            configuration,
        };
        assert!(
            Command::new("/usr/bin/systemctl")
                .arg("daemon-reload")
                .status()
                .unwrap()
                .success()
        );
        limits
    }

    pub(super) fn inventory(&self) -> Value {
        object([
            (
                "directory",
                Value::Str(self.directory.to_str().unwrap().into()),
            ),
            ("directory_identity", self.directory_identity.clone()),
            ("file_identity", self.file_identity.clone()),
            (
                "configuration",
                Value::Str(String::from_utf8(self.configuration.clone()).unwrap()),
            ),
        ])
    }

    pub(super) fn retire(self) {
        let metadata = fs::symlink_metadata(&self.directory).unwrap();
        assert!(metadata.is_dir() && metadata.uid() == 0);
        assert_eq!(identity(&metadata), self.directory_identity);
        let path = self.directory.join("90-fsm-workflow-memory.conf");
        let metadata = fs::symlink_metadata(&path).unwrap();
        assert!(metadata.is_file() && metadata.uid() == 0);
        assert_eq!(identity(&metadata), self.file_identity);
        assert_eq!(fs::read(&path).unwrap(), self.configuration);
        assert_eq!(fs::read_dir(&self.directory).unwrap().count(), 1);
        fs::remove_file(path).unwrap();
        fs::remove_dir(self.directory).unwrap();
        assert!(
            Command::new("/usr/bin/systemctl")
                .arg("daemon-reload")
                .status()
                .unwrap()
                .success()
        );
    }
}

pub(super) fn verify(fixture: &Fixture, domain: &Value, allocation: u64) {
    let receipt = read_value(
        &fixture
            .directory
            .join(format!("fixture-memory-{allocation}.json")),
        true,
    )
    .unwrap();
    assert_eq!(receipt.get("domain"), Some(domain));
    assert_eq!(text(&receipt, "memory_max").unwrap(), "1073741824");
    assert_eq!(text(&receipt, "memory_swap_max").unwrap(), "0");
}

pub(super) fn archive(fixture: &Fixture, staging: &Path) {
    let namespace = fixture
        .directory
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    let last = number(&fixture.counter(), "last_allocation").unwrap();
    assert!(last <= 4096);
    for allocation in 1..=last {
        let source = fixture
            .directory
            .join(format!("fixture-memory-{allocation}.json"));
        let metadata = match fs::symlink_metadata(&source) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => panic!("memory receipt metadata: {error}"),
        };
        assert!(metadata.is_file() && metadata.uid() == 0 && metadata.len() <= 4096);
        let mut destination = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(staging.join(format!("memory-{namespace}-{allocation}.json")))
            .unwrap();
        destination.write_all(&fs::read(source).unwrap()).unwrap();
        destination.sync_all().unwrap();
    }
}
