//! Table-driven argument parser.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::PathBuf;

use fsm_core::ident::suggest;

use crate::store::ErrorObj;

#[derive(Debug)]
pub struct CmdSpec {
    pub path: &'static [&'static str],
    pub positionals: &'static [&'static str],
    pub flags: &'static [&'static str],
    pub switches: &'static [&'static str],
    pub help: &'static str,
    pub run: fn(&mut Ctx, &Args) -> u8,
}

#[derive(Debug)]
pub struct Args {
    pub positionals: Vec<String>,
    pub flags: BTreeMap<&'static str, String>,
    pub switches: BTreeSet<&'static str>,
}

pub struct Ctx {
    pub data_dir: PathBuf,
    pub json: bool,
    pub color: bool,
    pub stdin: Option<String>,
}

impl Ctx {
    pub fn new(data_dir: PathBuf, json: bool, color: bool) -> Self {
        Self {
            data_dir,
            json,
            color,
            stdin: None,
        }
    }
}

pub fn platform_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Ok(h) = std::env::var("HOME") {
            return PathBuf::from(h).join("Library/Application Support/fsm");
        }
    }
    #[cfg(windows)]
    {
        if let Ok(a) = std::env::var("APPDATA") {
            return PathBuf::from(a).join("fsm");
        }
    }
    if let Ok(p) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(p).join("fsm");
    }
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h).join(".local/share/fsm");
    }
    PathBuf::from(".fsm")
}

pub fn default_data_dir() -> PathBuf {
    platform_data_dir()
}

pub fn resolve_data_dir(flag: Option<&str>) -> PathBuf {
    if let Some(p) = flag {
        return PathBuf::from(p);
    }
    if let Ok(p) = std::env::var("FSM_DATA_DIR") {
        return PathBuf::from(p);
    }
    platform_data_dir()
}

static REQ_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn reset_request_ids() {
    REQ_COUNTER.store(0, std::sync::atomic::Ordering::SeqCst);
}

pub fn default_request_id() -> String {
    let ms = crate::clock::now_ms();
    let c = REQ_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    format!("req-{ms}-{c}")
}

pub fn read_input(arg: &str) -> Result<String, ErrorObj> {
    read_input_from(arg, None)
}

pub fn read_input_from(arg: &str, stdin: Option<&str>) -> Result<String, ErrorObj> {
    if arg == "-" {
        if let Some(s) = stdin {
            return Ok(s.to_string());
        }
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .map_err(|e| ErrorObj::new("io/read", e.to_string()))?;
        Ok(s)
    } else if let Some(p) = arg.strip_prefix('@') {
        std::fs::read_to_string(p).map_err(|_| {
            ErrorObj::new("io/read", format!("cannot read {p}")).hint(format!("open {p}"))
        })
    } else if std::path::Path::new(arg).is_file() {
        std::fs::read_to_string(arg).map_err(|e| ErrorObj::new("io/read", e.to_string()))
    } else {
        Ok(arg.to_string())
    }
}

pub fn all_specs() -> Vec<&'static CmdSpec> {
    let mut v = Vec::new();
    v.extend(crate::cli::offline::SPECS.iter());
    v.extend(crate::cli::machine::SPECS.iter());
    v.extend(crate::cli::instance::SPECS.iter());
    v.extend(crate::cli::ops::SPECS.iter());
    v.extend(crate::cli::diagram::SPECS.iter());
    v.push(&SERVE);
    v
}

fn serve_run(_ctx: &mut Ctx, _args: &Args) -> u8 {
    match crate::mcp::serve::run() {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

static SERVE: CmdSpec = CmdSpec {
    path: &["serve"],
    positionals: &[],
    flags: &[],
    switches: &[],
    help: "Run the MCP stdio server",
    run: serve_run,
};

#[expect(clippy::print_stdout)]
pub fn print_help(specs: &[&CmdSpec]) {
    println!("{}", help_text(specs));
}

pub fn help_text(specs: &[&CmdSpec]) -> String {
    let mut s = String::from("fsm — deterministic statechart engine\n");
    for spec in specs {
        s.push_str(&format!("  {}  {}\n", spec.path.join(" "), spec.help));
        if !spec.flags.is_empty() {
            s.push_str(&format!("    flags: {}\n", spec.flags.join(", ")));
        }
        if !spec.switches.is_empty() {
            s.push_str(&format!("    switches: {}\n", spec.switches.join(", ")));
        }
    }
    s
}

fn usage(msg: &str) -> ErrorObj {
    ErrorObj::new("args", msg).hint(msg)
}

pub fn parse_with(
    specs: &[&'static CmdSpec],
    rest: &[String],
) -> Result<(&'static CmdSpec, Args), ErrorObj> {
    let mut best: Option<&'static CmdSpec> = None;
    for s in specs {
        if rest.len() >= s.path.len()
            && rest
                .iter()
                .take(s.path.len())
                .map(String::as_str)
                .eq(s.path.iter().copied())
        {
            if best.map(|b| b.path.len()).unwrap_or(0) < s.path.len() {
                best = Some(*s);
            }
        }
    }
    let Some(spec) = best else {
        let first = rest.first().map(String::as_str).unwrap_or("");
        let subs: Vec<String> = specs
            .iter()
            .filter(|s| s.path.first() == Some(&first))
            .map(|s| s.path.join(" "))
            .collect();
        if !subs.is_empty() {
            return Err(usage(&format!(
                "bare `{first}` needs a subcommand: {}",
                subs.join(", ")
            )));
        }
        let cands: Vec<&str> = specs
            .iter()
            .filter_map(|s| s.path.first().copied())
            .collect();
        let mut msg = format!("unknown command: {first}");
        if let Some(sug) = suggest(first, cands.iter().copied()) {
            msg.push_str(&format!("; did you mean {sug}?"));
        }
        return Err(usage(&msg));
    };
    if rest.len() == spec.path.len() && spec.positionals.is_empty() && !spec.path.is_empty() {
        // ok
    }
    let mut args = Args {
        positionals: Vec::new(),
        flags: BTreeMap::new(),
        switches: BTreeSet::new(),
    };
    let mut i = spec.path.len();
    while i < rest.len() {
        let a = &rest[i];
        if a == "-h" || a == "--help" {
            return Err(usage("help"));
        }
        if let Some(name) = a.strip_prefix("--") {
            if let Some((k, v)) = name.split_once('=') {
                if let Some(&flag) = spec.flags.iter().find(|f| **f == k) {
                    args.flags.insert(flag, v.to_string());
                } else {
                    return Err(unknown_flag(spec, k));
                }
            } else if spec.switches.contains(&name) {
                args.switches
                    .insert(spec.switches.iter().copied().find(|s| *s == name).unwrap());
            } else if spec.flags.contains(&name) {
                if i + 1 >= rest.len() {
                    return Err(usage(&format!("flag --{name} requires a value")));
                }
                let flag = spec.flags.iter().copied().find(|f| *f == name).unwrap();
                i += 1;
                args.flags.insert(flag, rest[i].clone());
            } else if name == "json" || name == "data-dir" {
                // consumed earlier
            } else {
                return Err(unknown_flag(spec, name));
            }
        } else if let Some(name) = a.strip_prefix('-') {
            if spec.flags.contains(&name) && i + 1 < rest.len() {
                let flag = spec.flags.iter().copied().find(|f| *f == name).unwrap();
                i += 1;
                args.flags.insert(flag, rest[i].clone());
            } else if spec.switches.contains(&name) {
                args.switches
                    .insert(spec.switches.iter().copied().find(|s| *s == name).unwrap());
            } else {
                return Err(unknown_flag(spec, name));
            }
        } else {
            args.positionals.push(a.clone());
        }
        i += 1;
    }
    if args.positionals.len() < spec.positionals.len() {
        let missing = spec.positionals[args.positionals.len()];
        return Err(usage(&format!("missing positional {missing}")));
    }
    if args.positionals.len() > spec.positionals.len() {
        return Err(usage("unexpected extra positional"));
    }
    Ok((spec, args))
}

fn unknown_flag(spec: &CmdSpec, name: &str) -> ErrorObj {
    let cands: Vec<&str> = spec
        .flags
        .iter()
        .copied()
        .chain(spec.switches.iter().copied())
        .collect();
    let mut msg = format!("unknown flag --{name}");
    if let Some(sug) = suggest(name, cands.iter().copied()) {
        msg.push_str(&format!("; did you mean --{sug}?"));
    }
    usage(&msg)
}

#[allow(clippy::print_stderr)]
pub fn dispatch(argv: Vec<String>) -> u8 {
    dispatch_specs(&all_specs(), argv)
}

pub fn dispatch_specs(specs: &[&'static CmdSpec], argv: Vec<String>) -> u8 {
    let mut rest: Vec<String> = argv.into_iter().skip(1).collect();
    if rest.is_empty() || (rest.len() == 1 && (rest[0] == "-h" || rest[0] == "--help")) {
        print_help(specs);
        return 0;
    }
    let mut json = false;
    let mut data_flag: Option<String> = None;
    let mut filtered = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        let a = &rest[i];
        if a == "--json" {
            json = true;
        } else if let Some(p) = a.strip_prefix("--data-dir=") {
            data_flag = Some(p.to_string());
        } else if a == "--data-dir" && i + 1 < rest.len() {
            i += 1;
            data_flag = Some(rest[i].clone());
        } else {
            filtered.push(a.clone());
        }
        i += 1;
    }
    rest = filtered;
    if rest.iter().any(|a| a == "-h" || a == "--help") && rest.len() <= 2 {
        let scoped: Vec<_> = specs
            .iter()
            .copied()
            .filter(|s| {
                rest.first()
                    .map(|t| s.path.first() == Some(&t.as_str()))
                    .unwrap_or(true)
            })
            .collect();
        print_help(&scoped);
        return 0;
    }
    match parse_with(specs, &rest) {
        Ok((spec, args)) => {
            let mut ctx = Ctx::new(
                resolve_data_dir(data_flag.as_deref()),
                json,
                std::env::var("NO_COLOR").is_err(),
            );
            (spec.run)(&mut ctx, &args)
        }
        Err(e) => {
            if e.message == "help" {
                print_help(specs);
                return 0;
            }
            let _ = writeln!(std::io::stderr(), "{}", e.message);
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop(_ctx: &mut Ctx, _args: &Args) -> u8 {
        0
    }

    static ADD: CmdSpec = CmdSpec {
        path: &["machine", "add"],
        positionals: &["spec"],
        flags: &["format"],
        switches: &["json"],
        help: "add",
        run: noop,
    };
    static ANALYZE: CmdSpec = CmdSpec {
        path: &["machine", "analyze"],
        positionals: &["machine"],
        flags: &["format"],
        switches: &["json"],
        help: "analyze",
        run: record_analyze,
    };
    static ANALYZE_HIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    fn record_analyze(_ctx: &mut Ctx, _args: &Args) -> u8 {
        ANALYZE_HIT.store(true, std::sync::atomic::Ordering::SeqCst);
        0
    }

    fn table() -> Vec<&'static CmdSpec> {
        vec![&ADD, &ANALYZE]
    }

    #[test]
    fn longest_prefix_dispatch() {
        let specs = table();
        let (spec, args) =
            parse_with(&specs, &["machine".into(), "analyze".into(), "x".into()]).unwrap();
        assert_eq!(spec.path, &["machine", "analyze"]);
        assert_eq!(args.positionals, ["x"]);
        let err = parse_with(&specs, &["machine".into()]).unwrap_err();
        assert_eq!(err.code, "args");
    }

    #[test]
    fn flag_forms() {
        let specs = table();
        let (_, a) = parse_with(
            &specs,
            &[
                "machine".into(),
                "add".into(),
                "s".into(),
                "--format=dot".into(),
            ],
        )
        .unwrap();
        assert_eq!(a.flags.get("format").map(String::as_str), Some("dot"));
        let (_, b) = parse_with(
            &specs,
            &[
                "machine".into(),
                "add".into(),
                "s".into(),
                "--format".into(),
                "dot".into(),
            ],
        )
        .unwrap();
        assert_eq!(b.flags.get("format").map(String::as_str), Some("dot"));
        let (_, c) = parse_with(
            &specs,
            &["machine".into(), "add".into(), "s".into(), "--json".into()],
        )
        .unwrap();
        assert!(c.switches.contains("json"));
        let err = parse_with(
            &specs,
            &[
                "machine".into(),
                "add".into(),
                "s".into(),
                "--format".into(),
            ],
        )
        .unwrap_err();
        assert!(err.message.contains("format"));
    }

    #[test]
    fn suggestions() {
        let specs = table();
        let err = parse_with(&specs, &["machin".into(), "add".into()]).unwrap_err();
        assert!(err.message.contains("machine"), "{}", err.message);
        let err = parse_with(
            &specs,
            &["machine".into(), "add".into(), "s".into(), "--jsno".into()],
        )
        .unwrap_err();
        assert!(err.message.contains("--json"), "{}", err.message);
    }

    #[test]
    fn help_from_the_table() {
        let specs = all_specs();
        let text = help_text(&specs);
        for s in &specs {
            assert!(
                text.contains(&s.path.join(" ")),
                "missing {} in help",
                s.path.join(" ")
            );
        }
    }

    #[test]
    fn read_input_three_forms() {
        assert_eq!(read_input_from("inline", None).unwrap(), "inline");
        assert_eq!(
            read_input_from("-", Some("from-stdin")).unwrap(),
            "from-stdin"
        );
        let p = std::env::temp_dir().join(format!("fsm-ri-{}", std::process::id()));
        std::fs::write(&p, "file-bytes").unwrap();
        let got = read_input_from(&format!("@{}", p.display()), None).unwrap();
        assert_eq!(got, "file-bytes");
        let err = read_input_from("@/no/such/fsm-missing", None).unwrap_err();
        assert!(err.code.starts_with("io"));
        assert!(err.message.contains("/no/such/fsm-missing") || err.hint.contains("/no/such"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn positional_arity() {
        let specs = table();
        let err = parse_with(&specs, &["machine".into(), "add".into()]).unwrap_err();
        assert!(err.message.contains("spec"), "{}", err.message);
        let err = parse_with(
            &specs,
            &["machine".into(), "add".into(), "a".into(), "extra".into()],
        )
        .unwrap_err();
        assert!(err.message.contains("extra") || err.message.contains("positional"));
    }

    #[test]
    fn serve_resolves() {
        let specs = all_specs();
        let (spec, _) = parse_with(&specs, &["serve".into()]).unwrap();
        assert_eq!(spec.path, &["serve"]);
    }

    #[test]
    fn dispatch_hits_analyze() {
        ANALYZE_HIT.store(false, std::sync::atomic::Ordering::SeqCst);
        let code = dispatch_specs(
            &table(),
            vec!["fsm".into(), "machine".into(), "analyze".into(), "x".into()],
        );
        assert_eq!(code, 0);
        assert!(ANALYZE_HIT.load(std::sync::atomic::Ordering::SeqCst));
    }
}
