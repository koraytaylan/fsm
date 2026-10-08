//! Independent kernel probe of the public preparation guard; no native closure claim.

pub(super) fn held_lease(
    namespace: &str,
    domain: &fsm_core::record::execution::NativeDomain,
) -> std::fs::File {
    let allocation = domain
        .to_value()
        .get("allocation")
        .unwrap()
        .as_num()
        .unwrap()
        .to_owned();
    let path = std::path::Path::new("/var/lib/fsm-containment")
        .join(namespace)
        .join("authority-1")
        .join(format!("owner-{allocation}.LOCK"));
    let probe = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    assert!(matches!(
        probe.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    probe
}
