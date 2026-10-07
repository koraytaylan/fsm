//! Process panic diagnostics and installation.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

static HOOK: AtomicBool = AtomicBool::new(false);

thread_local! {
    static ADAPTER_UNWIND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(target_os = "linux")]
pub(super) struct AdapterUnwind(bool);

#[cfg(target_os = "linux")]
impl AdapterUnwind {
    pub(super) fn enter() -> Self {
        Self(ADAPTER_UNWIND.with(|enabled| enabled.replace(true)))
    }
}

#[cfg(target_os = "linux")]
impl Drop for AdapterUnwind {
    fn drop(&mut self) {
        ADAPTER_UNWIND.with(|enabled| enabled.set(self.0));
    }
}

pub fn panic_text(info: &std::panic::PanicHookInfo<'_>) -> String {
    format!(
        "fsm panic: {info}\n{}",
        std::backtrace::Backtrace::force_capture()
    )
}

pub(super) fn install_panic_hook() {
    if HOOK
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        std::panic::set_hook(Box::new(|info| {
            if ADAPTER_UNWIND.with(std::cell::Cell::get) {
                // The catcher records the failure through bounded diagnostics;
                // no synchronous stderr write or process abort on this thread.
                return;
            }
            let _ = writeln!(std::io::stderr(), "{}", panic_text(info));
            std::process::abort();
        }));
    }
}
