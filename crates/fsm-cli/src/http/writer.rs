//! One process, many clients, one writer.
//!
//! The single-writer constraint stops being a limitation clients trip over
//! and becomes the serialization point they share.
//!
//! Plan 0015 tasks 7201 and 7202 fill this in.

/// The store every session's calls are serialized through.
pub struct SerializedWriter;

impl SerializedWriter {
    /// Run one call against the store, with every other caller waiting.
    pub fn with_store<T>(&self, _body: impl FnOnce(&mut crate::store::Store) -> T) -> T {
        unimplemented!("plan 0015 task 7201")
    }
}
