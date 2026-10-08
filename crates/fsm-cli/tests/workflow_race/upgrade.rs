//! New operator commands inspect and drain an original historical executor.
use super::*;

pub(super) fn operator(directory: &Directory) -> PathBuf {
    directory
        .1
        .as_ref()
        .and_then(|entry| entry.get("operator_cli"))
        .and_then(Value::as_str)
        .map_or_else(|| directory.executable(), PathBuf::from)
}

pub(super) fn inspect(directory: &Directory, phase: &str) {
    if directory.1.as_ref().and_then(|entry| entry.get("upgrade")) != Some(&Value::Bool(true)) {
        return;
    }
    assert_ne!(operator(directory), directory.executable());
    let before = Store::open_read_only(&directory.store()).unwrap();
    let records = before.records.clone();
    let count = before.state.execution.unresolved().count();
    assert_eq!(count, usize::from(phase == "retained"));
    let output = directory.0.join(format!("upgrade-{phase}.stdout"));
    let errors = directory.0.join(format!("upgrade-{phase}.stderr"));
    let mut child = Command::new(operator(directory))
        .args(["--json", "--data-dir"])
        .arg(directory.store())
        .args(["execute", "runs"])
        .stdin(Stdio::null())
        .stdout(fs::File::create(&output).unwrap())
        .stderr(fs::File::create(&errors).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            child.wait().unwrap();
            panic!("upgrade inspection exceeded its bound; original authority retained");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(status.success());
    for path in [&output, &errors] {
        assert!(fs::metadata(path).unwrap().len() <= 8192);
    }
    let response = parse(&fs::read(output).unwrap(), &JsonLimits::DEFAULT).unwrap();
    assert_eq!(response.get("runs").unwrap().as_arr().unwrap().len(), count);
    assert_eq!(
        Store::open_read_only(&directory.store()).unwrap().records,
        records
    );
    let transcript = object([("phase", Value::Str(phase.into())), ("response", response)]);
    #[allow(clippy::print_stdout)] // Retain only the bounded verified operator response.
    {
        println!(
            "\nFSM_NATIVE_UPGRADE_TRANSCRIPT {}",
            String::from_utf8(canon_bytes(&transcript)).unwrap()
        );
    }
}

pub(super) fn record_stop(directory: &Directory, response: &Value) {
    if directory.1.as_ref().and_then(|entry| entry.get("upgrade")) != Some(&Value::Bool(true)) {
        return;
    }
    let transcript = object([
        ("phase", Value::Str("stop".into())),
        ("response", response.clone()),
    ]);
    let bytes = canon_bytes(&transcript);
    assert!(bytes.len() <= 8192);
    #[allow(clippy::print_stdout)] // Bounded actual current-operator stop response.
    {
        println!(
            "\nFSM_NATIVE_UPGRADE_TRANSCRIPT {}",
            String::from_utf8(bytes).unwrap()
        );
    }
}
