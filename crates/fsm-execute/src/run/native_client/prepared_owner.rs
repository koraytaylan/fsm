//! Continuous operator ownership of one original prepared domain.

use fsm_core::record::execution::NativeDomain;
use std::fs::File;

/// Non-cloneable kernel guard; moving it preserves original preparation ownership.
/// Retain it through authenticated settlement or proved unclaimed cleanup.
pub struct NativePreparedOwner {
    domain: NativeDomain,
    _lease: File,
}

impl NativePreparedOwner {
    pub(super) fn acquire(domain: NativeDomain) -> Result<Self, String> {
        let lease = super::discovery::acquire_prepared_owner(&domain)?;
        Ok(Self {
            domain,
            _lease: lease,
        })
    }

    /// Original prepared metadata; a clone does not retain the ownership guard.
    pub fn domain(&self) -> &NativeDomain {
        &self.domain
    }
}
