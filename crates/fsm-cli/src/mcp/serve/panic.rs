//! Process panic diagnostics and installation.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

static HOOK: AtomicBool = AtomicBool::new(false);

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
            let _ = writeln!(std::io::stderr(), "{}", panic_text(info));
            std::process::abort();
        }));
    }
}
