//! Startup uses stdio's original writer observation and never upgrades a fallback.
use crate::{
    mcp::serve::{ServeMode, WriterUnavailable, open_writer},
    store::Store,
};
use std::path::Path;

pub(super) struct Opened {
    pub store: Option<Store>,
    pub diagnostic: Option<String>,
    pub mode_note: &'static str,
}

pub(super) fn open(dir: &Path, mode: &ServeMode) -> Opened {
    let opened = match mode {
        ServeMode::ReadOnly => Store::open_read_only(dir)
            .map_err(|error| WriterUnavailable::Unhealthy(Box::new(error))),
        ServeMode::Writer | ServeMode::Embedded(_) => open_writer(dir),
    };
    match opened {
        Ok(store) => Opened {
            store: Some(store),
            diagnostic: None,
            mode_note: "",
        },
        Err(WriterUnavailable::Contended(store)) => Opened {
            store: Some(*store),
            diagnostic: None,
            mode_note: "\nThis session is read-only because another writer holds the store.",
        },
        Err(WriterUnavailable::Unhealthy(error)) => Opened {
            store: None,
            diagnostic: Some(format!("{}: {}", error.code, error.message)),
            mode_note: "\nThe store is unavailable; use store_doctor to diagnose it.",
        },
    }
}
