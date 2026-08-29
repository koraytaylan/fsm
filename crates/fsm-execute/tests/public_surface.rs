//! The declared boundary of `fsm-execute`'s provisional public surface.
//!
//! `docs/API-POLICY.md` marks this crate provisional because it has no
//! outside-workspace acceptance check. That is honest, and on its own it is
//! unbounded: with nothing watching, "provisional" means whatever the crate
//! happens to expose in the release that ships. This test turns it into a
//! boundary somebody chose. Every public item is enumerated in
//! `tests/fixtures/public_surface.txt`; an addition or a removal fails here
//! until the inventory records it, so widening the surface is a decision in a
//! diff rather than an accumulation between releases.
//!
//! # Why a source scanner
//!
//! There is no other option available inside this workspace's charter.
//! `cargo public-api` is a third-party dependency and `CONTRIBUTING.md` allows
//! none; `cargo doc --output-format json` is nightly-only and CI runs stable
//! and the MSRV. Hand-rolling it is the same answer this project already gave
//! for JSON, SHA-256, JSON-RPC, and the MCP framing.
//!
//! # What this scanner cannot see
//!
//! It reads source text. It does not expand macros and it does not resolve
//! names, so two things are invisible to it and both are absent from
//! `fsm-execute` today — the note is what makes their arrival visible instead
//! of silent:
//!
//! * **Items produced by macro expansion.** A `macro_rules!` invocation that
//!   defines a public item contributes nothing to the inventory, so such an
//!   item could appear without failing this test.
//! * **Re-exports that widen visibility from a private module.** A
//!   `pub use` is recorded as a `reexport` line naming what it re-exports, and
//!   where it names an item of a private child module the scanner also emits
//!   that item's own members under the re-exporting module. It does not follow
//!   a re-export through more than that one hop, it does not resolve a glob
//!   (`pub use m::*`), which is recorded as a `reexport` of `*` and nothing
//!   more, and it does not resolve the members of a **renamed** re-export
//!   (`pub use m::A as B`) — the `reexport` line records the name a downstream
//!   writes, and `B`'s fields and methods are not enumerated under it.
//!
//! Trait implementations are also out of scope: `impl Display for Foo` is
//! public surface in the semantic sense, but it is not an item a downstream
//! names, and enumerating them would drown the inventory in derives. Inherent
//! `pub fn`s are enumerated as methods, which is what a caller actually writes.
//!
//! # Why only this crate
//!
//! `fsm-core` and `fsm-store` are deliberately left alone. Both already have
//! acceptance checks that fail when their surface regresses —
//! `crates/fsm-embed-acceptance` compiles against `fsm-core` from outside the
//! workspace, and the store's formats are pinned by goldens — which is a
//! stronger guarantee than an inventory. Adding a second, weaker mechanism
//! where a better one exists is the speculative generality `CONTRIBUTING.md`
//! warns against, so do not helpfully extend this test to them.

// The scanner is a sibling file rather than a section of this one: together
// they run past the thousand-line ceiling `scripts/oversized-files.sh`
// enforces, and they split cleanly at the seam between the mechanism and the
// boundary it enforces.
#[path = "public_surface/scanner.rs"]
mod scanner;

use scanner::{
    Scan, crate_root, differences, fixture_path, parse_inventory, public_surface, render,
    scan_source,
};

// ---------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------

#[test]
fn public_surface_matches_the_committed_inventory() {
    let observed = public_surface();
    if std::env::var("FSM_REGEN_FIXTURES").ok().as_deref() == Some("1") {
        std::fs::write(fixture_path(), render(&observed)).expect("inventory is writable");
        return;
    }
    let declared = parse_inventory(include_str!("fixtures/public_surface.txt"));
    let (added, removed) = differences(&declared, &observed);
    assert!(
        added.is_empty() && removed.is_empty(),
        "the public surface of fsm-execute moved.\n\
         added (not in the inventory):\n  {}\n\
         removed (in the inventory, not in the crate):\n  {}\n\
         If the change is intended, regenerate with \
         FSM_REGEN_FIXTURES=1 cargo test -p fsm-execute --test public_surface \
         and review the diff — widening a provisional surface is a decision, \
         not an accumulation.",
        added.join("\n  "),
        removed.join("\n  ")
    );
}

#[test]
fn an_added_public_item_fails_the_comparison() {
    let declared = public_surface();
    let mut observed = declared.clone();
    observed.push("fn fsm_execute::run::an_item_nobody_declared".to_string());
    let (added, removed) = differences(&declared, &observed);
    assert_eq!(added, vec!["fn fsm_execute::run::an_item_nobody_declared"]);
    assert!(removed.is_empty());
}

#[test]
fn a_removed_public_item_fails_the_comparison() {
    let declared = public_surface();
    let mut observed = declared.clone();
    let dropped = observed.pop().expect("the crate exposes something");
    let (added, removed) = differences(&declared, &observed);
    assert!(added.is_empty());
    assert_eq!(removed, vec![dropped]);
}

#[test]
fn the_inventory_covers_fields_and_variants_not_only_types() {
    let inventory = public_surface();
    assert!(
        inventory.iter().any(|line| line.starts_with("field ")),
        "no public struct field was enumerated"
    );
    assert!(
        inventory.iter().any(|line| line.starts_with("variant ")),
        "no public enum variant was enumerated"
    );
    // `HandlerKind` is an enum with variants and `Handler` is a struct with
    // public fields; both are named here so the assertion pins a real type
    // rather than the existence of some type.
    assert!(
        inventory
            .iter()
            .any(|line| line == "enum fsm_execute::config::HandlerKind"),
        "the known public enum is missing"
    );
    assert!(
        inventory
            .iter()
            .any(|line| line.starts_with("variant fsm_execute::config::HandlerKind::")),
        "the known public enum's variants are missing"
    );
    assert!(
        inventory
            .iter()
            .any(|line| line.starts_with("field fsm_execute::config::HandlerSpec::")),
        "the known public struct's fields are missing"
    );
    // A struct-like enum variant carries fields that no `pub` keyword marks,
    // and they are public because the enum is. Missing them would understate
    // the surface in exactly the direction that hides a break.
    assert!(
        inventory
            .iter()
            .any(|line| line == "variant fsm_execute::config::HandlerKind::Mcp"),
        "a struct-like enum variant is missing"
    );
    assert!(
        inventory
            .iter()
            .any(|line| line == "field fsm_execute::config::HandlerKind::Mcp::tool"),
        "a struct-like enum variant's fields are missing"
    );
}

#[test]
fn an_impl_trait_argument_does_not_swallow_the_function_that_carries_it() {
    // `impl Trait` is legal in argument and return position, and treating that
    // token as an impl block made the function itself invisible: `pub fn
    // converse(stdin: impl Write, ..)` was missing from the inventory entirely,
    // so renaming or deleting it — a break for anyone hosting the loop — left
    // this gate green.
    let source = "
        pub fn takes(one: impl Write, two: &str) -> u8 { 0 }
        pub fn returns() -> impl Iterator<Item = u8> { [].into_iter() }
        pub struct Owner;
        impl Owner { pub fn method(&self) {} }
        impl Display for Owner { pub fn fmt(&self) {} }
    ";
    let mut scan = Scan::default();
    scan_source(source, "probe", &mut scan);
    assert_eq!(
        scan.items,
        vec![
            "fn probe::takes".to_string(),
            "fn probe::returns".to_string(),
            "struct probe::Owner".to_string(),
            "method probe::Owner::method".to_string(),
        ],
        "an `impl` in a signature was mistaken for an impl block"
    );
    // The real crate carries such a function; assert the inventory holds it.
    assert!(
        public_surface()
            .iter()
            .any(|line| line == "fn fsm_execute::mcp_client::converse"),
        "the crate's own `impl Trait`-taking function is missing"
    );
}

#[test]
fn a_generic_parameter_list_does_not_split_the_type_that_carries_it() {
    // The comma in `<A, B>` sits at no parenthesis depth. Splitting there
    // recorded the name and then opened the body with a fragment of a type as
    // its head, so every field was walked as an opaque block and contributed
    // nothing — a type present in the inventory with none of its members.
    let source = "
        pub struct Pair<A, B> { pub left: A, pub right: B }
        pub enum Either<A, B> { Left(A), Right(B) }
        pub fn zip<A, B>(a: A, b: B) {}
    ";
    let mut scan = Scan::default();
    scan_source(source, "probe", &mut scan);
    assert_eq!(
        scan.items,
        vec![
            "struct probe::Pair".to_string(),
            "field probe::Pair::left".to_string(),
            "field probe::Pair::right".to_string(),
            "enum probe::Either".to_string(),
            "variant probe::Either::Left".to_string(),
            "variant probe::Either::Right".to_string(),
            "fn probe::zip".to_string(),
        ]
    );
}

#[test]
fn a_tuple_structs_public_fields_are_positional_items() {
    // `.0` is what a downstream names, so it is what a downstream breaks on.
    let source = "
        pub struct Wrapper(pub u8, u8, pub BTreeMap<String, u8>);
        pub struct Unit;
    ";
    let mut scan = Scan::default();
    scan_source(source, "probe", &mut scan);
    assert_eq!(
        scan.items,
        vec![
            "struct probe::Wrapper".to_string(),
            "field probe::Wrapper::0".to_string(),
            "field probe::Wrapper::2".to_string(),
            "struct probe::Unit".to_string(),
        ],
        "a private tuple field was listed, or a public one was not"
    );
}

#[test]
fn a_nested_use_tree_expands_to_one_line_per_leaf() {
    // Splitting on the first brace and then on every comma produced corrupted
    // lines rather than a refusal — silent garbage in a file whose whole
    // purpose is to be read as a diff.
    let source = "
        pub use a::{b::{c, d}, e};
        pub use f::g as h;
        pub use i::*;
    ";
    let mut scan = Scan::default();
    scan_source(source, "probe", &mut scan);
    assert_eq!(
        scan.items,
        vec![
            "reexport probe::c from a::b".to_string(),
            "reexport probe::d from a::b".to_string(),
            "reexport probe::e from a".to_string(),
            "reexport probe::h from f".to_string(),
            "reexport probe::* from i".to_string(),
        ]
    );
}

#[test]
fn a_re_export_out_of_a_private_module_lands_under_the_public_path() {
    // `run.rs` declares `mod pipeline;` privately and then `pub use
    // pipeline::{Pipeline, SettleOutcome};`. Those two items are public and
    // their own module is not, so an inventory that scanned only reachable
    // modules would miss them and an inventory that scanned every file would
    // claim the rest of `pipeline` too. Both directions are asserted here.
    let inventory = public_surface();
    for expected in [
        "reexport fsm_execute::run::Pipeline from pipeline",
        "struct fsm_execute::run::Pipeline",
        "method fsm_execute::run::Pipeline::settle",
        "enum fsm_execute::run::SettleOutcome",
        "variant fsm_execute::run::SettleOutcome::Advanced",
    ] {
        assert!(
            inventory.iter().any(|line| line == expected),
            "the re-exported item is missing from the inventory: {expected}"
        );
    }
    assert!(
        !inventory
            .iter()
            .any(|line| line.contains("fsm_execute::run::pipeline")),
        "the private module itself leaked into the inventory"
    );
}

#[test]
fn the_scan_is_deterministic_and_independent_of_source_order() {
    let first = public_surface();
    let second = public_surface();
    assert_eq!(first, second, "two scans of one crate disagreed");
    let mut sorted = first.clone();
    sorted.sort();
    assert_eq!(first, sorted, "the inventory is not in sorted order");
    let mut deduplicated = first.clone();
    deduplicated.dedup();
    assert_eq!(first, deduplicated, "the inventory holds a duplicate line");
}

#[test]
fn regeneration_reproduces_the_committed_bytes_on_an_unchanged_crate() {
    if std::env::var("FSM_REGEN_FIXTURES").ok().as_deref() == Some("1") {
        // The gate test is rewriting the file in this very process; reading it
        // here would race the write rather than check anything.
        return;
    }
    let committed = std::fs::read_to_string(fixture_path()).expect("inventory is readable");
    assert_eq!(
        render(&public_surface()),
        committed,
        "regenerating the inventory would rewrite it, so the committed file is stale"
    );
}

#[test]
fn restricted_visibility_is_not_public() {
    let source = "
        pub(crate) struct Internal { pub visible: u8 }
        pub(super) fn helper() {}
        pub (crate) const SPACED: u8 = 0;
        pub struct Exposed { pub kept: u8, pub(crate) hidden: u8 }
    ";
    let mut scan = Scan::default();
    scan_source(source, "probe", &mut scan);
    assert_eq!(
        scan.items,
        vec![
            "struct probe::Exposed".to_string(),
            "field probe::Exposed::kept".to_string(),
        ]
    );
}

#[test]
fn a_pub_inside_a_comment_or_a_string_is_not_an_item() {
    let source = r###"
        // pub fn commented_out() {}
        /* pub struct BlockCommented; */
        /** pub enum DocCommented {} */
        pub fn real(message: &str) -> String {
            let _ = "pub struct InsideAString;";
            let _ = r#"pub fn inside_a_raw_string() {}"#;
            let _ = 'p';
            message.to_string()
        }
    "###;
    let mut scan = Scan::default();
    scan_source(source, "probe", &mut scan);
    assert_eq!(scan.items, vec!["fn probe::real".to_string()]);
}

#[test]
fn a_private_module_contributes_nothing_until_it_is_re_exported() {
    let source = "
        mod hidden { pub struct Buried { pub field: u8 } pub fn also_buried() {} }
        pub mod shown { pub struct Seen; }
    ";
    let mut scan = Scan::default();
    scan_source(source, "probe", &mut scan);
    assert_eq!(
        scan.items,
        vec![
            "mod probe::shown".to_string(),
            "struct probe::shown::Seen".to_string(),
        ]
    );
    assert!(scan.private_children["probe"].contains("hidden"));
}

#[test]
fn the_module_doc_states_what_the_scanner_cannot_see() {
    // The limits are the difference between a boundary and a claim. Asserting
    // them here means the note cannot be deleted while this test still passes.
    let own_source = include_str!("public_surface.rs");
    // Collapse the wrapping: a sentence split across two `//!` lines is the
    // same sentence, and an assertion that breaks when a paragraph is rewrapped
    // teaches the next reader to delete the assertion.
    let doc: String = own_source
        .lines()
        .take_while(|line| line.starts_with("//!") || line.is_empty())
        .flat_map(|line| line.trim_start_matches("//!").split_whitespace())
        .collect::<Vec<_>>()
        .join(" ");
    for expected in [
        "Items produced by macro expansion",
        "Re-exports that widen visibility from a private module",
        "does not follow a re-export through more than that one hop",
        "are deliberately left alone",
    ] {
        assert!(
            doc.contains(expected),
            "the module doc no longer states: {expected}"
        );
    }
}

#[test]
fn the_api_policy_still_calls_this_crate_provisional() {
    // A later edit must not quietly promote the crate while this test passes:
    // the inventory bounds a provisional surface, it does not stabilise one.
    let policy = std::fs::read_to_string(crate_root().join("../../docs/API-POLICY.md"))
        .expect("API-POLICY.md is readable");
    let row = policy
        .lines()
        .find(|line| line.contains("fsm-execute") && line.to_lowercase().contains("provisional"))
        .expect("API-POLICY.md no longer marks fsm-execute provisional");
    assert!(
        row.contains("public_surface"),
        "the fsm-execute row does not say where the enumerated boundary lives: {row}"
    );
}
