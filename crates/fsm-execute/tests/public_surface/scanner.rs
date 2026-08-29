//! The source scanner behind `public_surface.rs`.
//!
//! Read that file's module doc first: it states what this scanner is for, what
//! it cannot see, and why the crate has no other option. This file is the
//! mechanism — blank the comments and literals, walk the braces, and record
//! what each context declares — kept beside the gate rather than inside it
//! because the two are read for different reasons.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The inventory line for one public item: `<kind> <path>`, sorted as text.
pub(super) type Inventory = Vec<String>;

pub(super) fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub(super) fn fixture_path() -> PathBuf {
    crate_root().join("tests/fixtures/public_surface.txt")
}

// ---------------------------------------------------------------------------
// Lexical preparation
// ---------------------------------------------------------------------------

/// Blank out comments, string literals, and character literals, preserving
/// every byte offset and newline so the remainder still parses positionally.
///
/// A `pub` inside a doc comment or inside an error message is not a public
/// item, and a scanner that cannot tell the difference silently overstates the
/// surface it claims to bound.
fn blank_comments_and_literals(source: &str) -> String {
    let bytes: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut index = 0;
    while index < bytes.len() {
        let ch = bytes[index];
        let next = bytes.get(index + 1).copied();
        if ch == '/' && next == Some('/') {
            while index < bytes.len() && bytes[index] != '\n' {
                out.push(' ');
                index += 1;
            }
            continue;
        }
        if ch == '/' && next == Some('*') {
            let mut depth = 0usize;
            while index < bytes.len() {
                if bytes[index] == '/' && bytes.get(index + 1) == Some(&'*') {
                    depth += 1;
                    out.push(' ');
                    out.push(' ');
                    index += 2;
                    continue;
                }
                if bytes[index] == '*' && bytes.get(index + 1) == Some(&'/') {
                    depth -= 1;
                    out.push(' ');
                    out.push(' ');
                    index += 2;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                out.push(if bytes[index] == '\n' { '\n' } else { ' ' });
                index += 1;
            }
            continue;
        }
        if ch == 'r'
            && matches!(next, Some('"') | Some('#'))
            && let Some(consumed) = blank_raw_string(&bytes, index, &mut out)
        {
            index = consumed;
            continue;
        }
        if ch == '"' {
            out.push(' ');
            index += 1;
            while index < bytes.len() {
                if bytes[index] == '\\' {
                    out.push(' ');
                    out.push(' ');
                    index += 2;
                    continue;
                }
                let done = bytes[index] == '"';
                out.push(if bytes[index] == '\n' { '\n' } else { ' ' });
                index += 1;
                if done {
                    break;
                }
            }
            continue;
        }
        if ch == '\'' && is_character_literal(&bytes, index) {
            out.push(' ');
            index += 1;
            while index < bytes.len() {
                if bytes[index] == '\\' {
                    out.push(' ');
                    out.push(' ');
                    index += 2;
                    continue;
                }
                let done = bytes[index] == '\'';
                out.push(' ');
                index += 1;
                if done {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
        index += 1;
    }
    out
}

/// Blank `r"…"` and `r#"…"#`, returning the index just past the literal.
fn blank_raw_string(bytes: &[char], start: usize, out: &mut String) -> Option<usize> {
    let mut index = start + 1;
    let mut hashes = 0usize;
    while bytes.get(index) == Some(&'#') {
        hashes += 1;
        index += 1;
    }
    if bytes.get(index) != Some(&'"') {
        return None;
    }
    for _ in start..=index {
        out.push(' ');
    }
    index += 1;
    while index < bytes.len() {
        if bytes[index] == '"' {
            let closes = (1..=hashes).all(|offset| bytes.get(index + offset) == Some(&'#'));
            if closes {
                for _ in 0..=hashes {
                    out.push(' ');
                }
                return Some(index + hashes + 1);
            }
        }
        out.push(if bytes[index] == '\n' { '\n' } else { ' ' });
        index += 1;
    }
    Some(index)
}

/// A `'` opens a character literal only when it is not a lifetime. A lifetime
/// is `'` followed by an identifier that is not itself closed by a quote.
fn is_character_literal(bytes: &[char], start: usize) -> bool {
    match bytes.get(start + 1) {
        Some('\\') => true,
        Some(_) => bytes.get(start + 2) == Some(&'\''),
        None => false,
    }
}

// ---------------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------------

/// What the brace we are inside was opened by. Only these contexts can hold a
/// public item; anything else is opaque and its contents are skipped entirely,
/// which is what keeps expression punctuation out of the parse.
#[derive(Debug, Clone)]
enum Context {
    Module(String),
    Struct(String),
    Enum(String),
    /// The body of a struct-like enum variant. Every field in one is public
    /// when the enum is, so unlike `Struct` it looks for no `pub` keyword.
    VariantFields(String),
    Trait(String),
    Impl(String),
    Opaque,
}

/// One module's declarations of its child modules, and whether each is public.
type ModuleChildren = BTreeMap<String, BTreeSet<String>>;

#[derive(Default)]
pub(super) struct Scan {
    pub(super) items: Vec<String>,
    /// `module path -> names it declares as `pub mod``.
    public_children: ModuleChildren,
    /// `module path -> names it declares as a private `mod``.
    pub(super) private_children: ModuleChildren,
    /// `(re-exporting module, target module, imported name)`.
    reexports: Vec<(String, String, String)>,
}

pub(super) fn scan_source(source: &str, module: &str, scan: &mut Scan) {
    let code = blank_comments_and_literals(source);
    let mut stack: Vec<Context> = vec![Context::Module(module.to_string())];
    let mut opaque_depth = 0usize;
    let mut head = String::new();
    let mut group_depth = 0usize;
    // `use a::{b, c};` braces group names; they open no item and their commas
    // separate no fields. Without this the whole statement is shredded and a
    // re-export — the one construct that can widen visibility out of a private
    // module — is the thing the scanner silently stops seeing.
    let mut use_braces = 0usize;
    for ch in code.chars() {
        if opaque_depth > 0 {
            match ch {
                '{' => opaque_depth += 1,
                '}' => {
                    opaque_depth -= 1;
                    if opaque_depth == 0 {
                        stack.pop();
                    }
                }
                _ => {}
            }
            continue;
        }
        match ch {
            '(' | '[' => {
                group_depth += 1;
                head.push(ch);
            }
            ')' | ']' => {
                group_depth = group_depth.saturating_sub(1);
                head.push(ch);
            }
            '{' if use_braces > 0 || opens_a_use_statement(&head) => {
                use_braces += 1;
                head.push(ch);
            }
            '}' if use_braces > 0 => {
                use_braces -= 1;
                head.push(ch);
            }
            '{' => {
                let context = open_context(head.trim(), stack.last().unwrap(), scan);
                head.clear();
                group_depth = 0;
                if matches!(context, Context::Opaque) {
                    opaque_depth = 1;
                }
                stack.push(context);
            }
            '}' => {
                record_item(head.trim(), stack.last().unwrap(), scan);
                head.clear();
                group_depth = 0;
                if stack.len() > 1 {
                    stack.pop();
                }
            }
            ';' | ',' if group_depth == 0 && use_braces == 0 => {
                record_item(head.trim(), stack.last().unwrap(), scan);
                head.clear();
            }
            _ => head.push(ch),
        }
    }
}

/// Whether the fragment so far is the head of a `use` statement.
fn opens_a_use_statement(head: &str) -> bool {
    let (_, tokens) = declaration(head);
    tokens
        .first()
        .is_some_and(|token| token == "use" || token.starts_with("use "))
}

/// Strip leading attributes and return `(is_public, remaining_tokens)`.
///
/// `pub(crate)` and `pub(super)` are **not** public: they are visible inside
/// the workspace and invisible to a downstream, which is the whole distinction
/// this inventory exists to record.
fn declaration(head: &str) -> (bool, Vec<String>) {
    let mut rest = head.trim();
    while let Some(after) = rest.strip_prefix('#') {
        let after = after.trim_start().strip_prefix('!').unwrap_or(after);
        let Some(open) = after.find('[') else { break };
        let mut depth = 0usize;
        let mut end = None;
        for (offset, ch) in after.char_indices().skip(open) {
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(offset + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        match end {
            Some(end) => rest = after[end..].trim_start(),
            None => break,
        }
    }
    let tokens: Vec<String> = rest.split_whitespace().map(str::to_string).collect();
    let is_public = tokens
        .first()
        .is_some_and(|first| first == "pub" || first.starts_with("pub<"));
    let restricted = tokens
        .first()
        .is_some_and(|first| first.starts_with("pub(") || rest.starts_with("pub ("));
    let tokens = if is_public || restricted {
        tokens[1..].to_vec()
    } else {
        tokens
    };
    (is_public && !restricted, tokens)
}

/// The identifier a keyword introduces, with generics and bounds trimmed.
fn name_after(tokens: &[String], keyword: &str) -> Option<String> {
    let position = tokens.iter().position(|token| token == keyword)?;
    let raw = tokens.get(position + 1)?;
    let name: String = raw
        .chars()
        .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// The first identifier of a fragment, with any generics or payload trimmed.
fn leading_identifier(tokens: &[String]) -> Option<String> {
    let name: String = tokens
        .first()?
        .chars()
        .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// The type an inherent `impl` block is for. The keyword itself may carry
/// generics (`impl<'a> Watcher<'a>`), so the token is matched by prefix.
fn impl_target(tokens: &[String]) -> Option<String> {
    let position = tokens
        .iter()
        .position(|token| token == "impl" || token.starts_with("impl<"))?;
    leading_identifier(&tokens[position + 1..])
}

fn qualified(context: &Context, name: &str) -> String {
    match context {
        Context::Module(path) => format!("{path}::{name}"),
        Context::Struct(path)
        | Context::Enum(path)
        | Context::VariantFields(path)
        | Context::Trait(path)
        | Context::Impl(path) => format!("{path}::{name}"),
        Context::Opaque => name.to_string(),
    }
}

fn open_context(head: &str, parent: &Context, scan: &mut Scan) -> Context {
    if head.contains("cfg(test)") {
        return Context::Opaque;
    }
    let (is_public, tokens) = declaration(head);
    if let Context::Enum(path) = parent {
        // `Variant { field: T, .. }` — a struct-like variant. The variant is
        // public because the enum is, and so is every field it carries.
        if let Some(name) = leading_identifier(&tokens) {
            scan.items.push(format!("variant {path}::{name}"));
            return Context::VariantFields(format!("{path}::{name}"));
        }
        return Context::Opaque;
    }
    let Context::Module(module) = parent else {
        // A brace inside a type or an impl is a body, a where-clause block, or
        // an expression. None of them introduces an item.
        if let Context::Impl(_) | Context::Trait(_) = parent
            && let Some(name) = name_after(&tokens, "fn")
        {
            if is_public || matches!(parent, Context::Trait(_)) {
                scan.items
                    .push(format!("method {}", qualified(parent, &name)));
            }
            return Context::Opaque;
        }
        return Context::Opaque;
    };
    if let Some(name) = name_after(&tokens, "mod") {
        let child = format!("{module}::{name}");
        let bucket = if is_public {
            &mut scan.public_children
        } else {
            &mut scan.private_children
        };
        bucket.entry(module.clone()).or_default().insert(name);
        if !is_public {
            return Context::Opaque;
        }
        scan.items.push(format!("mod {child}"));
        return Context::Module(child);
    }
    if let Some(name) = impl_target(&tokens) {
        // `impl Trait for Type` names the type after `for`; a bare `impl Type`
        // names it after the keyword. Trait implementations are out of scope,
        // so only the inherent form contributes methods.
        if tokens.iter().any(|token| token == "for") {
            return Context::Opaque;
        }
        return Context::Impl(format!("{module}::{name}"));
    }
    for (keyword, kind) in [("struct", "struct"), ("enum", "enum"), ("trait", "trait")] {
        if let Some(name) = name_after(&tokens, keyword) {
            if !is_public {
                return Context::Opaque;
            }
            let path = format!("{module}::{name}");
            scan.items.push(format!("{kind} {path}"));
            return match keyword {
                "struct" => Context::Struct(path),
                "enum" => Context::Enum(path),
                _ => Context::Trait(path),
            };
        }
    }
    if let Some(name) = name_after(&tokens, "fn") {
        if is_public {
            scan.items.push(format!("fn {module}::{name}"));
        }
        return Context::Opaque;
    }
    Context::Opaque
}

fn record_item(head: &str, context: &Context, scan: &mut Scan) {
    if head.is_empty() || head.contains("cfg(test)") {
        return;
    }
    let (is_public, tokens) = declaration(head);
    match context {
        Context::Enum(path) => {
            // Every variant of a public enum is public; there is no `pub` to
            // look for, so the first identifier of the fragment is the name.
            if let Some(first) = tokens.first() {
                let name: String = first
                    .chars()
                    .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                    .collect();
                if !name.is_empty() && name.chars().next().is_some_and(char::is_uppercase) {
                    scan.items.push(format!("variant {path}::{name}"));
                }
            }
        }
        Context::VariantFields(path) => {
            if let Some(name) = leading_identifier(&tokens)
                && tokens.first().is_some_and(|token| token.ends_with(':'))
            {
                scan.items.push(format!("field {path}::{name}"));
            }
        }
        Context::Struct(path) => {
            if is_public && let Some(first) = tokens.first() {
                let name: String = first
                    .chars()
                    .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                    .collect();
                if !name.is_empty() {
                    scan.items.push(format!("field {path}::{name}"));
                }
            }
        }
        Context::Trait(path) => {
            for (keyword, kind) in [("fn", "method"), ("type", "type"), ("const", "const")] {
                if let Some(name) = name_after(&tokens, keyword) {
                    scan.items.push(format!("{kind} {path}::{name}"));
                }
            }
        }
        Context::Impl(path) => {
            if is_public {
                for (keyword, kind) in [("fn", "method"), ("const", "const")] {
                    if let Some(name) = name_after(&tokens, keyword) {
                        scan.items.push(format!("{kind} {path}::{name}"));
                    }
                }
            }
        }
        Context::Module(module) => {
            if !is_public {
                return;
            }
            if tokens.first().is_some_and(|token| token.starts_with("use")) {
                record_reexport(head, module, scan);
                return;
            }
            for (keyword, kind) in [
                ("mod", "mod"),
                ("struct", "struct"),
                ("enum", "enum"),
                ("trait", "trait"),
                ("type", "type"),
                ("const", "const"),
                ("static", "static"),
                ("fn", "fn"),
            ] {
                if let Some(name) = name_after(&tokens, keyword) {
                    if keyword == "mod" {
                        scan.public_children
                            .entry(module.clone())
                            .or_default()
                            .insert(name.clone());
                    }
                    scan.items.push(format!("{kind} {module}::{name}"));
                    return;
                }
            }
        }
        Context::Opaque => {}
    }
}

/// Record a `pub use`, and remember which module it drew each name from so a
/// re-export out of a private module can be resolved into real items.
fn record_reexport(head: &str, module: &str, scan: &mut Scan) {
    let Some(after) = head.split_once("use ") else {
        return;
    };
    let path = after.1.trim();
    let (prefix, names) = match path.split_once('{') {
        Some((prefix, names)) => (
            prefix.trim().trim_end_matches("::"),
            names.trim_end_matches('}').to_string(),
        ),
        None => match path.rsplit_once("::") {
            Some((prefix, name)) => (prefix, name.to_string()),
            None => ("", path.to_string()),
        },
    };
    for name in names.split(',') {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        scan.items
            .push(format!("reexport {module}::{name} from {prefix}"));
        let target = if prefix.starts_with("crate::") {
            prefix.replacen("crate", crate_module(), 1)
        } else if prefix.is_empty() {
            continue;
        } else {
            format!("{module}::{prefix}")
        };
        scan.reexports
            .push((module.to_string(), target, name.to_string()));
    }
}

fn crate_module() -> &'static str {
    "fsm_execute"
}

// ---------------------------------------------------------------------------
// Assembly
// ---------------------------------------------------------------------------

/// Every `.rs` file under a directory, in a deterministic order.
fn source_files(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![directory.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// `src/lib.rs` is the crate root; `src/a/b.rs` is `crate::a::b`.
fn module_path_of(source_root: &Path, file: &Path) -> String {
    let relative = file.strip_prefix(source_root).unwrap();
    let mut segments = vec![crate_module().to_string()];
    let components: Vec<String> = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect();
    for (index, component) in components.iter().enumerate() {
        let last = index + 1 == components.len();
        if last {
            let stem = component.trim_end_matches(".rs");
            if stem != "lib" && stem != "mod" {
                segments.push(stem.to_string());
            }
        } else {
            segments.push(component.clone());
        }
    }
    segments.join("::")
}

/// Scan the crate and return its public inventory, sorted and deduplicated.
pub(super) fn public_surface() -> Inventory {
    let source_root = crate_root().join("src");
    let mut scan = Scan::default();
    let mut per_module: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in source_files(&source_root) {
        let module = module_path_of(&source_root, &file);
        let before = scan.items.len();
        let source = std::fs::read_to_string(&file).expect("source file is readable");
        scan_source(&source, &module, &mut scan);
        per_module
            .entry(module)
            .or_default()
            .extend(scan.items[before..].iter().cloned());
    }
    let reachable = reachable_modules(&scan);
    let mut inventory: Vec<String> = Vec::new();
    for (module, items) in &per_module {
        if reachable.contains(module) {
            inventory.extend(items.iter().cloned());
        }
    }
    // A `pub use` naming an item of a private child module makes that item
    // public under the re-exporting path. Emit its members there, one hop only.
    for (module, target, name) in &scan.reexports {
        if reachable.contains(target) || !reachable.contains(module) {
            continue;
        }
        let Some(items) = per_module.get(target) else {
            continue;
        };
        let owned = format!("{target}::{name}");
        for item in items {
            let Some((kind, path)) = item.split_once(' ') else {
                continue;
            };
            if path == owned || path.starts_with(&format!("{owned}::")) {
                let tail = path.strip_prefix(target).unwrap_or(path);
                inventory.push(format!("{kind} {module}{tail}"));
            }
        }
    }
    inventory.sort();
    inventory.dedup();
    inventory
}

pub(super) fn render(inventory: &Inventory) -> String {
    let mut text = String::new();
    for line in inventory {
        text.push_str(line);
        text.push('\n');
    }
    text
}

/// The comparison itself, kept separate from the scan so both directions can
/// be proved against a synthetic surface rather than a throwaway public item.
pub(super) fn differences(
    declared: &Inventory,
    observed: &Inventory,
) -> (Vec<String>, Vec<String>) {
    let declared_set: BTreeSet<&String> = declared.iter().collect();
    let observed_set: BTreeSet<&String> = observed.iter().collect();
    let added = observed_set
        .difference(&declared_set)
        .map(|line| (*line).clone())
        .collect();
    let removed = declared_set
        .difference(&observed_set)
        .map(|line| (*line).clone())
        .collect();
    (added, removed)
}

fn reachable_modules(scan: &Scan) -> BTreeSet<String> {
    let mut reachable = BTreeSet::new();
    reachable.insert(crate_module().to_string());
    // Modules form a tree, so one pass per level suffices; iterate to a fixed
    // point rather than assume a depth.
    loop {
        let before = reachable.len();
        for (parent, children) in &scan.public_children {
            if reachable.contains(parent) {
                for child in children {
                    reachable.insert(format!("{parent}::{child}"));
                }
            }
        }
        if reachable.len() == before {
            break;
        }
    }
    reachable
}

pub(super) fn parse_inventory(text: &str) -> Inventory {
    text.lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}
