//! Shared numeric and size limits.

pub const MAX_STATES: usize = 256;
pub const MAX_NESTING: u32 = 12;
pub const MAX_HISTORY: usize = 32;
pub const MAX_EVENTS: usize = 128;
pub const MAX_ENUMS: usize = 32;
pub const MAX_VARIANTS: usize = 64;
pub const MAX_TRANSITIONS: usize = 2048;
pub const MAX_TRANSITIONS_PER_CELL: usize = 32;
pub const MAX_CTX_VARS: usize = 64;
pub const MAX_FIELDS: usize = 32;
pub const MAX_SETS_PER_BLOCK: usize = 32;
pub const MAX_EMITS_PER_BLOCK: usize = 8;
pub const MAX_INVARIANTS: usize = 64;
pub const MAX_DEF_BYTES: usize = 256 * 1024;
