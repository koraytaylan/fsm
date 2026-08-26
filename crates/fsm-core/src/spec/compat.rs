use crate::json::Value;
use crate::limits;
use crate::machine::CompiledMachine;

use super::compile::compile_with_compatibility;
use super::parse::parse_machine;
use super::validate::DefinitionCompatibility;
use super::{Finding, MachineSpec};

fn semantics_eq(a: &MachineSpec, b: &MachineSpec) -> bool {
    a.format == b.format
        && a.name == b.name
        && a.description == b.description
        && a.enums == b.enums
        && a.context == b.context
        && a.events == b.events
        && a.effects == b.effects
        && a.topology == b.topology
        && a.deadlines == b.deadlines
        && a.on_unhandled == b.on_unhandled
        && a.transitions == b.transitions
        && a.invariants == b.invariants
}

pub(super) fn identity_document(spec: &MachineSpec) -> Value {
    if let Some(src) = &spec.source {
        if let Ok(parsed) = parse_machine(src) {
            if semantics_eq(&parsed, spec) {
                return src.clone();
            }
        }
    }
    spec.to_value()
}

/// Canonical bytes and machine id from one accepted definition value.
pub fn accepted_identity(def: &Value) -> (Vec<u8>, String) {
    (
        crate::canon::canon_bytes(def),
        crate::hashes::machine_id(def),
    )
}

/// Compile a definition using the accepted source document as the identity input.
pub fn compile_accepted(source: &Value) -> Result<CompiledMachine, Vec<Finding>> {
    compile_accepted_with_compatibility(source, DefinitionCompatibility::Current)
}

/// [`compile_accepted`] with the invoked machines in hand, so a
/// `$done.invoke.<slot>` payload types against the child's declarations.
pub fn compile_accepted_with_catalogue(
    source: &Value,
    catalogue: &super::Catalogue,
) -> Result<CompiledMachine, Vec<Finding>> {
    if crate::canon::canon_bytes(source).len() > limits::MAX_DEF_BYTES {
        return Err(vec![Finding::err(
            "def/limit_bytes",
            "/",
            "definition exceeds 256 KiB",
            "shrink the definition",
        )]);
    }
    let spec = super::parse_machine(source)?;
    super::compile_with_catalogue(spec, catalogue)
}

fn compile_accepted_with_compatibility(
    source: &Value,
    compatibility: DefinitionCompatibility,
) -> Result<CompiledMachine, Vec<Finding>> {
    if crate::canon::canon_bytes(source).len() > limits::MAX_DEF_BYTES {
        return Err(vec![Finding::err(
            "def/limit_bytes",
            "/",
            "definition exceeds 256 KiB",
            "shrink the document",
        )]);
    }
    let spec = parse_machine(source)?;
    compile_with_compatibility(spec, compatibility)
}

/// Compile a definition using the legacy persistence compatibility rules.
///
/// This unchecked primitive skips the current aggregate expression ceiling.
/// For sequential definitions without deadlines, it also preserves the old
/// admission of ownerless, child-bearing, terminal, or initial-bearing history
/// pseudostates. All other structural, parsing, and typing checks remain
/// enforced. It does not authenticate a genesis or prove that `source` was
/// previously persisted. Callers MUST restrict it to machine material reached
/// through a complete hash-verified historical-genesis fold, or a snapshot
/// bound to such a journal. Definition admission MUST use [`compile_accepted`].
#[doc(hidden)]
pub fn compile_accepted_historical_unchecked(
    source: &Value,
) -> Result<CompiledMachine, Vec<Finding>> {
    compile_accepted_with_compatibility(source, DefinitionCompatibility::HistoricalPersistence)
}

pub fn load_machine_json(bytes: &[u8]) -> Result<MachineSpec, Vec<Finding>> {
    if bytes.len() > limits::MAX_DEF_BYTES {
        return Err(vec![Finding::err(
            "def/limit_bytes",
            "/",
            "definition exceeds 256 KiB",
            "shrink the document",
        )]);
    }
    let v = crate::json::parse(bytes, &crate::json::JsonLimits::DEFAULT)
        .map_err(|e| vec![Finding::err("def/shape", "/", e.message, "fix the JSON")])?;
    parse_machine(&v)
}
