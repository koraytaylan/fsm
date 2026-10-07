//! Immutable helper startup, with a private per-caller fixture factory.

use super::{InlineRequest, PreparedRequest};

pub(super) struct Startup {
    prepared: PreparedRequest,
    #[cfg(test)]
    factory: Option<Factory>,
}

impl Startup {
    pub(super) fn new(prepared: PreparedRequest) -> Self {
        Self {
            prepared,
            #[cfg(test)]
            factory: FACTORY.with(|factory| factory.borrow_mut().take()),
        }
    }

    pub(super) fn deadline(&self) -> std::time::Instant {
        self.prepared.deadline
    }

    pub(super) fn start(self) -> Result<InlineRequest, String> {
        #[cfg(test)]
        if let Some(factory) = self.factory {
            return factory(self.prepared);
        }
        InlineRequest::start_prepared(self.prepared)
    }
}

#[cfg(test)]
type Factory = Box<dyn FnOnce(PreparedRequest) -> Result<InlineRequest, String> + Send>;

#[cfg(test)]
thread_local! {
    static FACTORY: std::cell::RefCell<Option<Factory>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(super) struct FixtureFactory(Option<Factory>);

#[cfg(test)]
impl FixtureFactory {
    pub(super) fn install(
        factory: impl FnOnce(PreparedRequest) -> Result<InlineRequest, String> + Send + 'static,
    ) -> Self {
        Self(FACTORY.with(|current| current.replace(Some(Box::new(factory)))))
    }
}

#[cfg(test)]
impl Drop for FixtureFactory {
    fn drop(&mut self) {
        FACTORY.with(|current| current.replace(self.0.take()));
    }
}
