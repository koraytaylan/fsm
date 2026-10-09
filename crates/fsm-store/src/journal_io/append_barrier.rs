//! Test-only IPC barriers at real append boundaries; no synthetic persistence errors.

use fsm_core::json::{JsonLimits, Value, parse};

thread_local! {
    static AFTER_WRITE: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

pub(super) fn after_write(action: impl FnOnce() + 'static) {
    AFTER_WRITE.with(|slot| *slot.borrow_mut() = Some(Box::new(action)));
}

pub(super) fn wait(line: &[u8], phase: &str) {
    if phase == "after-write" {
        let action = AFTER_WRITE.with(|slot| slot.borrow_mut().take());
        if let Some(action) = action {
            action();
        }
    }
    let Some(directory) = std::env::var_os("FSM_PRIVATE_APPEND_BARRIER_DIRECTORY") else {
        return;
    };
    if std::env::var("FSM_PRIVATE_APPEND_BARRIER_PHASE").as_deref() != Ok(phase) {
        return;
    }
    let record = parse(line, &JsonLimits::DEFAULT).unwrap();
    let selected = std::env::var("FSM_PRIVATE_APPEND_BARRIER_KIND").unwrap();
    if record.get("kind").and_then(Value::as_str) != Some(selected.as_str()) {
        return;
    }
    let directory = std::path::PathBuf::from(directory);
    std::fs::write(
        directory.join("append-barrier.tmp"),
        fsm_core::canon::canon_bytes(&record),
    )
    .unwrap();
    std::fs::rename(
        directory.join("append-barrier.tmp"),
        directory.join("append-barrier.json"),
    )
    .unwrap();
    // The parent owns this process and must kill/wait it; no API can return
    // past the selected boundary and no normal Drop may publish a snapshot.
    loop {
        std::thread::park();
    }
}
