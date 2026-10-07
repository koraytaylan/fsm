//! The frozen ownership inventory uses production's native owner on Linux.
//! Other platforms retain the writer-only boundary, without claiming native proof.

use super::super::Handle;
use crate::{clock::FixedClock, store::Store};

#[cfg(target_os = "linux")]
pub(super) struct AcceptanceOwner {
    native: super::super::native::NativeOwner<FixedClock>,
    pub mailbox: std::sync::Arc<super::super::mailbox::Mailbox>,
}

#[cfg(target_os = "linux")]
impl AcceptanceOwner {
    pub fn run(self) {
        use fsm_execute::service::ExecutorPhase;
        let mut exit = self.native.run();
        assert!(exit.failure.is_none());
        assert_eq!(exit.shutdown.phase, ExecutorPhase::Stopped);
        assert!(exit.shutdown.inventory_complete);
        assert!(exit.shutdown.helpers_retired);
        assert!(exit.shutdown.writer_released);
        assert!(exit.driver.store_mut().is_none());
        assert_eq!(exit.diagnostics.dropped(), 0);
    }
}

#[cfg(target_os = "linux")]
pub(super) fn new(store: Store, clock: FixedClock) -> (AcceptanceOwner, Handle) {
    use crate::mcp::notify::diagnostic_output::DiagnosticOutput;
    use fsm_execute::{config::HandlerTable, service::OwnedNativeExecutor};
    let driver = OwnedNativeExecutor::new(store, HandlerTable::default()).unwrap();
    let diagnostics = DiagnosticOutput::start(std::io::sink()).unwrap();
    let (native, handle) = super::super::native::NativeOwner::new(
        driver,
        clock,
        diagnostics,
        std::time::Duration::from_millis(250),
        10000,
    )
    .unwrap();
    let mailbox = std::sync::Arc::clone(&handle.mailbox);
    (AcceptanceOwner { native, mailbox }, handle)
}

#[cfg(not(target_os = "linux"))]
pub(super) fn new(store: Store, clock: FixedClock) -> (super::super::Owner<FixedClock>, Handle) {
    super::super::Owner::new(store, clock)
}
