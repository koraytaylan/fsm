//! Privileged native identity prototype; not production admission or a backend.
//! Only a validated, root-owned, task-specific /run namespace can be used.

use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Record {
    id: u64,
    inode: u64,
    boot: String,
    phase: String,
}

fn read(path: &Path) -> Result<String, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4096 {
        return Err("bounded record exceeded".into());
    }
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

fn put(path: &Path, value: &str) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
    use std::io::Write;
    file.write_all(value.as_bytes())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(temporary, path).map_err(|e| e.to_string())?;
    File::open(path.parent().ok_or("missing parent")?)
        .map_err(|e| e.to_string())?
        .sync_all()
        .map_err(|e| e.to_string())
}

fn encode(record: &Record) -> String {
    format!(
        "identity/1\n{}\n{}\n{}\n{}\n",
        record.id, record.inode, record.boot, record.phase
    )
}

fn decode(value: &str) -> Result<Record, String> {
    let lines: Vec<_> = value.lines().collect();
    if lines.len() != 5
        || lines[0] != "identity/1"
        || !matches!(lines[4], "prepared" | "armed" | "closing" | "closed")
    {
        return Err("invalid identity record".into());
    }
    let id: u64 = lines[1].parse().map_err(|_| "invalid identity counter")?;
    let inode: u64 = lines[2].parse().map_err(|_| "invalid domain inode")?;
    if id == 0 || inode == 0 || lines[3].len() != 36 {
        return Err("invalid identity".into());
    }
    Ok(Record {
        id,
        inode,
        boot: lines[3].into(),
        phase: lines[4].into(),
    })
}

fn checked_root(path: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.uid() != 0 || meta.mode() & 0o022 != 0 {
        return Err("namespace must be a protected root-owned directory".into());
    }
    Ok(())
}

fn unit(namespace: &str, id: u64) -> String {
    format!("fsm-containment-identity-{namespace}-{id}.service")
}

fn domain(namespace: &str, id: u64) -> PathBuf {
    Path::new("/sys/fs/cgroup/system.slice").join(unit(namespace, id))
}

fn observe(namespace: &str, record: &Record, boot: &str) -> String {
    let path = domain(namespace, record.id);
    if record.boot != boot {
        return "unknown-boot".into();
    }
    if record.phase == "closed" {
        return if path.exists() {
            "closed-alias-present"
        } else {
            "closed"
        }
        .into();
    }
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && meta.uid() == 0 && meta.ino() == record.inode => {
            record.phase.clone()
        }
        Ok(_) => "unknown-identity".into(),
        Err(_) => "unknown-missing".into(),
    }
}

fn status(command: &mut Command) -> Result<(), String> {
    let status = command.status().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("native command failed: {status}"))
    }
}

pub(super) fn run(base: &Path, operation: &str) -> Result<(), String> {
    if fs::metadata("/proc/self").map_err(|e| e.to_string())?.uid() != 0 {
        return Err("native identity helper requires provisioned root authority".into());
    }
    let name = base
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("invalid namespace")?;
    let namespace = name
        .strip_prefix("fsm-containment-identity-")
        .ok_or("invalid namespace")?;
    if base.parent() != Some(Path::new("/run"))
        || namespace.len() != 32
        || !namespace
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err("only a unique task-owned /run namespace is accepted".into());
    }
    checked_root(base)?;
    let data = base.join("data");
    // Only separate provisioner setup initializes authority. Missing state
    // cannot silently become a new namespace with recycled counters.
    checked_root(&data)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(data.join("LOCK"))
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    let boot = read(Path::new("/proc/sys/kernel/random/boot_id"))?
        .trim()
        .to_owned();
    let counter_path = data.join("counter");
    let active_path = data.join("active");
    let record_path = |id| data.join(format!("run-{id}"));
    let fields: Vec<_> = operation.split(':').collect();
    if fields.len() != 1 && fields.len() != 4 {
        return Err("identity operations require the complete counter/inode/boot handle".into());
    }
    let action = fields[0];
    let record = if fields.len() == 4 {
        let id: u64 = fields[1].parse().map_err(|_| "invalid supplied identity")?;
        let inode: u64 = fields[2].parse().map_err(|_| "invalid supplied inode")?;
        let record = decode(&read(&record_path(id))?)?;
        if record.id != id || record.inode != inode || record.boot != fields[3] {
            return Err("supplied identity does not match protected authority".into());
        }
        Some(record)
    } else {
        None
    };
    let response = match action {
        "allocate" if record.is_none() => {
            let mut last_active = 0;
            if active_path.exists() {
                let id: u64 = read(&active_path)?
                    .trim()
                    .parse()
                    .map_err(|_| "invalid active counter")?;
                let old = decode(&read(&record_path(id))?)?;
                if old.id != id {
                    return Err("active record routing mismatch".into());
                }
                last_active = id;
                if observe(namespace, &old, &boot) != "closed" {
                    return Err("unresolved native identity refuses successor allocation".into());
                }
            }
            let previous: u64 = read(&counter_path)?
                .trim()
                .parse()
                .map_err(|_| "missing or invalid monotonic authority")?;
            if previous < last_active {
                return Err("counter authority rolled backward".into());
            }
            let id = previous.checked_add(1).ok_or("counter exhausted")?;
            // Burn the counter durably before creating the domain: a crash
            // cannot cause this logical identity to be allocated a second time.
            if record_path(id).exists() {
                return Err("counter rollback cannot overwrite a historical identity".into());
            }
            put(&counter_path, &format!("{id}\n"))?;
            let path = domain(namespace, id);
            fs::create_dir(&path).map_err(|e| e.to_string())?;
            let events = read(&path.join("cgroup.events"))?;
            if !events.lines().any(|line| line == "populated 0") {
                return Err("new domain is not empty".into());
            }
            let record = Record {
                id,
                inode: fs::metadata(path).map_err(|e| e.to_string())?.ino(),
                boot: boot.clone(),
                phase: "prepared".into(),
            };
            put(&record_path(id), &encode(&record))?;
            put(&active_path, &format!("{id}\n"))?;
            encode(&record)
        }
        "inspect" => {
            let record = record.ok_or("inspection needs an identity")?;
            format!(
                "{}\n{}",
                observe(namespace, &record, &boot),
                encode(&record)
            )
        }
        "launch-hold" => {
            let mut record = record.ok_or("launch needs an identity")?;
            if observe(namespace, &record, &boot) != "prepared" {
                return Err("stale launch refused".into());
            }
            let active: u64 = read(&active_path)?
                .trim()
                .parse()
                .map_err(|_| "invalid active identity")?;
            if active != record.id {
                return Err("inactive identity refuses launch".into());
            }
            record.phase = "armed".into();
            put(&record_path(record.id), &encode(&record))?;
            let fixture = base.join("fixture");
            let meta = fs::symlink_metadata(&fixture).map_err(|e| e.to_string())?;
            if !meta.is_file() || meta.uid() != 0 || meta.mode() & 0o022 != 0 {
                return Err("untrusted fixture".into());
            }
            let work = base.join("work");
            let mut command = Command::new("/usr/bin/systemd-run");
            command
                .args(["--quiet", "--collect", "--no-block"])
                .arg(format!("--unit={}", unit(namespace, record.id)));
            for property in [
                "DynamicUser=yes",
                "ProtectControlGroups=yes",
                "ProtectHome=yes",
                "NoNewPrivileges=yes",
                "CapabilityBoundingSet=",
                "Delegate=no",
                "KillMode=control-group",
                "ExitType=cgroup",
                "KillSignal=SIGKILL",
                "Restart=no",
                "TimeoutStopSec=2s",
                "RuntimeMaxSec=15s",
                "PrivateTmp=no",
                "RestrictNamespaces=yes",
            ] {
                command.arg(format!("--property={property}"));
            }
            command
                .arg(format!("--property=ReadWritePaths={}", work.display()))
                .arg(format!(
                    "--setenv=FSM_LIFECYCLE_PROBE_DIRECTORY={}",
                    work.display()
                ))
                .arg("--setenv=FSM_LIFECYCLE_PROBE_MODE=kill")
                .arg("--setenv=FSM_LIFECYCLE_PROBE_CONTAINED=1")
                .arg(fixture)
                .args(["native_fixture", "--exact", "--nocapture"]);
            status(&mut command)?;
            // Kill probes wait for this barrier, so utility-client death before
            // handoff is explicitly not covered by this partial experiment.
            put(
                &base.join("helper-pid"),
                &format!("{}\n", std::process::id()),
            )?;
            let watchdog = Instant::now();
            while !work.join("release-helper").exists()
                && watchdog.elapsed() < Duration::from_secs(20)
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            encode(&record)
        }
        "close" => {
            let mut record = record.ok_or("close needs an identity")?;
            let observed = observe(namespace, &record, &boot);
            if observed == "closed" {
                return put(&base.join("response"), &encode(&record));
            }
            if !matches!(observed.as_str(), "prepared" | "armed" | "closing") {
                return Err("unknown identity refuses native cleanup".into());
            }
            record.phase = "closing".into();
            put(&record_path(record.id), &encode(&record))?;
            let path = domain(namespace, record.id);
            fs::write(path.join("cgroup.freeze"), b"1\n").map_err(|e| e.to_string())?;
            let deadline = Instant::now();
            while !read(&path.join("cgroup.events"))?
                .lines()
                .any(|line| line == "frozen 1")
            {
                if deadline.elapsed() > Duration::from_secs(2) {
                    return Err("freeze deadline".into());
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            fs::write(path.join("cgroup.kill"), b"1\n").map_err(|e| e.to_string())?;
            let _ = Command::new("/usr/bin/systemctl")
                .args(["stop", &unit(namespace, record.id)])
                .status();
            if path.exists() {
                if !read(&path.join("cgroup.events"))?
                    .lines()
                    .any(|line| line == "populated 0")
                {
                    return Err("domain still populated".into());
                }
                fs::remove_dir(&path).map_err(|e| e.to_string())?;
            }
            record.phase = "closed".into();
            put(&record_path(record.id), &encode(&record))?;
            encode(&record)
        }
        _ => return Err("unknown native identity operation".into()),
    };
    put(&base.join("response"), &response)
}
