//! Privileged native identity prototype; not production admission or a backend.
//! Only a validated, root-owned, task-specific /run namespace can be used.

use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Record {
    id: u64,
    inode: u64,
    boot: String,
    phase: String,
}

pub(super) fn read(path: &Path) -> Result<String, String> {
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

pub(super) fn put(path: &Path, value: &str) -> Result<(), String> {
    put_mode(path, value, false)
}

pub(super) fn put_public(path: &Path, value: &str) -> Result<(), String> {
    put_mode(path, value, true)
}

fn put_mode(path: &Path, value: &str, public: bool) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
    use std::io::Write;
    file.write_all(value.as_bytes())
        .map_err(|e| e.to_string())?;
    if public {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o444))
            .map_err(|e| e.to_string())?;
    }
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

fn publish_grant(base: &Path, record: &Record) -> Result<(), String> {
    let path = base.join("grants").join(record.id.to_string());
    checked_root(&base.join("grants"))?;
    put_public(&path, &encode(record))
}

/// Runs inside the unprivileged domain before any fixture handler code.
/// Missing, revoked or mismatched grants fail before external markers.
pub(super) fn gate(work: &Path, operation: &str) -> Result<(), String> {
    if operation.len() > 128 {
        return Err("gate handle exceeds bound".into());
    }
    let base = work.parent().ok_or("missing gate namespace")?;
    checked_root(base)?;
    checked_root(&base.join("grants"))?;
    let fields: Vec<_> = operation.split(':').collect();
    if fields.len() != 3 {
        return Err("gate needs full identity".into());
    }
    let id: u64 = fields[0].parse().map_err(|_| "bad gate counter")?;
    let inode: u64 = fields[1].parse().map_err(|_| "bad gate inode")?;
    let path = base.join("grants").join(id.to_string());
    let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.uid() != 0 || meta.mode() & 0o022 != 0 {
        return Err("untrusted gate authorization".into());
    }
    let grant = decode(&read(&path)?)?;
    if grant.id != id || grant.inode != inode || grant.boot != fields[2] || grant.phase != "armed" {
        return Err("gate authorization revoked or mismatched".into());
    }
    let boot = read(Path::new("/proc/sys/kernel/random/boot_id"))?
        .trim()
        .to_owned();
    let namespace = base
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_prefix("fsm-containment-identity-"))
        .ok_or("invalid gate namespace")?;
    let expected = format!("0::/system.slice/{}\n", unit(namespace, id));
    if boot != grant.boot
        || read(Path::new("/proc/self/cgroup"))? != expected
        || fs::metadata(domain(namespace, id))
            .map_err(|e| e.to_string())?
            .ino()
            != inode
    {
        return Err("gate native enrollment does not match authorization".into());
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

pub(super) fn verify_namespace_domains(base: &Path) -> Result<(), String> {
    let namespace = base
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("fsm-containment-identity-"))
        .ok_or("invalid native namespace")?;
    if base.parent() != Some(Path::new("/run"))
        || namespace.len() != 32
        || !namespace
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("invalid native namespace".into());
    }
    let prefix = format!("fsm-containment-identity-{namespace}-");
    let boot = read(Path::new("/proc/sys/kernel/random/boot_id"))?;
    let data = base.join("data");
    let entries = fs::read_dir("/sys/fs/cgroup/system.slice")
        .map_err(|_| "native namespace inventory unavailable")?;
    for (index, entry) in entries.enumerate() {
        if index >= 4096 {
            return Err("native namespace inventory exceeds bound".into());
        }
        let entry = entry.map_err(|_| "native namespace inventory unavailable")?;
        let name = entry.file_name();
        if !name.as_bytes().starts_with(prefix.as_bytes()) {
            continue;
        }
        let suffix = std::str::from_utf8(&name.as_bytes()[prefix.len()..])
            .map_err(|_| "unknown native namespace domain refuses authority")?;
        let verify = || -> Result<(), String> {
            let id: u64 = suffix
                .strip_suffix(".service")
                .ok_or("name")?
                .parse()
                .map_err(|_| "id")?;
            if suffix != format!("{id}.service") {
                return Err("noncanonical domain name".into());
            }
            let record = decode(&read(&data.join(format!("run-{id}")))?)?;
            let active: u64 = read(&data.join("active"))?
                .trim()
                .parse()
                .map_err(|_| "active")?;
            let counter: u64 = read(&data.join("counter"))?
                .trim()
                .parse()
                .map_err(|_| "counter")?;
            if id != record.id
                || active != id
                || counter < id
                || !matches!(
                    observe(namespace, &record, boot.trim()).as_str(),
                    "prepared" | "armed" | "closing"
                )
            {
                return Err("identity".into());
            }
            Ok(())
        };
        verify().map_err(|_| "unknown native namespace domain refuses authority")?;
    }
    Ok(())
}

fn status(command: &mut Command) -> Result<(), String> {
    let mut child = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now();
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) if status.success() => return Ok(()),
            Some(status) => return Err(format!("native command failed: {status}")),
            None if deadline.elapsed() >= Duration::from_secs(3) => {
                let _ = child.kill();
                // A trusted utility can also remain uninterruptible. Do not
                // turn its timeout into an unbounded blocking reap, or infer
                // that manager jobs or handler ownership disappeared.
                let reap = Instant::now();
                while reap.elapsed() < Duration::from_millis(250) {
                    if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                return Err("native utility deadline; ownership remains unresolved".into());
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}

pub(super) fn run(base: &Path, operation: &str) -> Result<(), String> {
    if operation.len() > 256 {
        return Err("operation exceeds bound".into());
    }
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
    let lock_deadline = Instant::now();
    while lock.try_lock().is_err() {
        if lock_deadline.elapsed() >= Duration::from_millis(250) {
            return Err("native authority busy or unavailable".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
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
            verify_namespace_domains(base)?;
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
            // A broker's restrictive socket umask also affects prepared cgroup
            // controls; permit handler inspection without granting writes.
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
            for name in ["memory.max", "memory.swap.max"] {
                fs::set_permissions(path.join(name), fs::Permissions::from_mode(0o644))
                    .map_err(|e| e.to_string())?;
            }
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
        "launch" | "launch-hold" => {
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
            publish_grant(base, &record)?;
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
            if base.join("hold-entry").exists() {
                command.arg(format!(
                    "--property=ExecStartPre=/usr/bin/env FSM_LIFECYCLE_PROBE_MODE=identity-pre-entry {} native_fixture --exact --nocapture",
                    fixture.display()
                ));
            }
            for property in [
                "MemoryMax=1G",
                "MemorySwapMax=0",
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
                .arg(format!(
                    "--setenv=FSM_LIFECYCLE_PROBE_MODE=identity-gate:{}:{}:{}",
                    record.id, record.inode, record.boot
                ))
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
            while action == "launch-hold"
                && !work.join("release-helper").exists()
                && watchdog.elapsed() < Duration::from_secs(20)
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            encode(&record)
        }
        "close" | "close-hold" | "close-finalize-hold" => {
            let mut record = record.ok_or("close needs an identity")?;
            let observed = observe(namespace, &record, &boot);
            if observed == "closed" {
                return put(&base.join("response"), &encode(&record));
            }
            if observed == "unknown-missing" && record.phase == "closing" {
                // Absence alone is never closure evidence. Only a matching
                // protected receipt written after actual native cleanup can
                // recover the interrupted final tombstone publication.
                let receipt = read(&data.join(format!("closure-{}", record.id)))
                    .and_then(|value| decode(&value))
                    .map_err(|_| "unknown identity refuses native cleanup")?;
                let grant = decode(&read(&base.join("grants").join(record.id.to_string()))?)?;
                for evidence in [&receipt, &grant] {
                    if evidence.id != record.id
                        || evidence.inode != record.inode
                        || evidence.boot != record.boot
                    {
                        return Err("closure evidence identity mismatch".into());
                    }
                }
                if receipt.phase != "closed"
                    || !matches!(grant.phase.as_str(), "closing" | "closed")
                {
                    return Err("closure evidence phase mismatch".into());
                }
                record.phase = "closed".into();
                put(&record_path(record.id), &encode(&record))?;
                publish_grant(base, &record)?;
                return put(&base.join("response"), &encode(&record));
            }
            if !matches!(observed.as_str(), "prepared" | "armed" | "closing") {
                return Err("unknown identity refuses native cleanup".into());
            }
            record.phase = "closing".into();
            // Revoke entry first: a death after publishing closing but before
            // persisting the private phase leaves an armed record that can be
            // closed again, never a closing record with an armed public grant.
            publish_grant(base, &record)?;
            if action == "close-hold" {
                put(&base.join("revocation-ready"), "revoked\n")?;
                let watchdog = Instant::now();
                while !base.join("release-close").exists()
                    && watchdog.elapsed() < Duration::from_secs(20)
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
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
            let _ = status(
                Command::new("/usr/bin/systemctl").args(["stop", &unit(namespace, record.id)]),
            );
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
            put(
                &data.join(format!("closure-{}", record.id)),
                &encode(&record),
            )?;
            if action == "close-finalize-hold" {
                put(&base.join("closure-ready"), "native closure recorded\n")?;
                let watchdog = Instant::now();
                while !base.join("release-finalize").exists()
                    && watchdog.elapsed() < Duration::from_secs(20)
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            put(&record_path(record.id), &encode(&record))?;
            publish_grant(base, &record)?;
            encode(&record)
        }
        _ => return Err("unknown native identity operation".into()),
    };
    put(&base.join("response"), &response)
}
