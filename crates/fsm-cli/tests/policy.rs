//! Machine-checked purity gates for `fsm-core` source.

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Violation {
    line: usize,
    token: String,
}

const BANNED: &[&str] = &[
    "f32",
    "f64",
    "SystemTime",
    "Instant",
    "HashMap",
    "HashSet",
    "std::fs",
    "std::net",
    "std::process",
    "rand",
    "unsafe",
];

fn strip_line_comment(line: &str) -> &str {
    match line.find("//") {
        Some(p) => &line[..p],
        None => line,
    }
}

fn scan(source: &str) -> Vec<Violation> {
    let mut out = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        let lineno = idx + 1;
        let has_allow = line.contains("POLICY_ALLOW:");
        let has_bare = line.contains("POLICY_ALLOW") && !has_allow;
        if has_bare {
            out.push(Violation {
                line: lineno,
                token: "POLICY_ALLOW".into(),
            });
        }
        if has_allow {
            continue;
        }
        let code = strip_line_comment(line);
        for tok in BANNED {
            if code.contains(tok) {
                out.push(Violation {
                    line: lineno,
                    token: (*tok).into(),
                });
            }
        }
    }
    out
}

fn walk_core(dir: &Path, violations: &mut Vec<(String, Violation)>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            walk_core(&path, violations);
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        for v in scan(&src) {
            violations.push((path.display().to_string(), v));
        }
    }
}

#[test]
fn scanner_banned_token_on_code_line() {
    let v = scan("let x: HashMap<u8, u8> = todo!();\n");
    assert_eq!(
        v,
        vec![Violation {
            line: 1,
            token: "HashMap".into()
        }]
    );
}

#[test]
fn scanner_comment_is_clean() {
    assert!(scan("// HashMap is banned\n").is_empty());
}

#[test]
fn scanner_string_literal_is_violation() {
    let v = scan("let s = \"HashMap\";\n");
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].token, "HashMap");
}

#[test]
fn scanner_allow_with_reason_is_clean() {
    assert!(scan("let x: HashMap<u8, u8> = todo!(); // POLICY_ALLOW: test fixture\n").is_empty());
}

#[test]
fn scanner_bare_allow_is_violation() {
    let v = scan("let x = 1; // POLICY_ALLOW\n");
    assert_eq!(
        v,
        vec![Violation {
            line: 1,
            token: "POLICY_ALLOW".into()
        }]
    );
}

#[test]
fn scanner_two_violations() {
    let v = scan("let a: f32 = 0.0;\nlet b: f64 = 0.0;\n");
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].line, 1);
    assert_eq!(v[1].line, 2);
}

#[test]
fn core_src_has_zero_violations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fsm-core/src");
    let mut violations = Vec::new();
    walk_core(&root, &mut violations);
    assert!(
        violations.is_empty(),
        "policy violations: {:?}",
        violations
            .iter()
            .map(|(p, v)| format!("{p}:{} {}", v.line, v.token))
            .collect::<Vec<_>>()
    );
}
