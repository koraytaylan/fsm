//! Internal worker-body unwind permission; no name or caller can arm it.

thread_local! {
    static ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(super) struct WorkerUnwind(bool);

impl WorkerUnwind {
    pub(super) fn enter() -> Self {
        Self(ENABLED.with(|enabled| enabled.replace(true)))
    }
}

impl Drop for WorkerUnwind {
    fn drop(&mut self) {
        ENABLED.with(|enabled| enabled.set(self.0));
    }
}

pub(super) fn is_isolated() -> bool {
    ENABLED.with(std::cell::Cell::get)
}

#[cfg(test)]
mod tests;
