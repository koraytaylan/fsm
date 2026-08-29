use fsm_core::diagram::{InstanceOverlay, dot, mermaid};
use fsm_core::json::Value;
use std::collections::BTreeSet;

use crate::args::{Args, CmdSpec, Ctx};
use crate::render::{emit_error, emit_success};
use crate::store::{ErrorObj, Store};

fn diagram(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(r) = args.positionals.first() else {
        return emit_error(ctx, &ErrorObj::new("args", "machine diagram <machine>"));
    };
    let fmt = args
        .flags
        .get("format")
        .map(String::as_str)
        .unwrap_or("mermaid");
    if fmt != "mermaid" && fmt != "dot" {
        return emit_error(ctx, &ErrorObj::new("args", "unknown --format"));
    }
    let store = match Store::open_read_only(&ctx.data_dir) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    let overlay = if let Some(iid) = args.flags.get("instance") {
        match store.state.instances.get(iid) {
            Some(inst) => {
                let current_leaves = match &inst.configuration {
                    fsm_core::machine::ActiveConfiguration::Sequential { leaf } => {
                        BTreeSet::from([leaf.clone()])
                    }
                    fsm_core::machine::ActiveConfiguration::Parallel { leaves } => {
                        leaves.values().cloned().collect()
                    }
                };
                Some(InstanceOverlay {
                    visited: current_leaves.clone(),
                    current_leaves,
                })
            }
            None => return emit_error(ctx, &ErrorObj::new("req/instance_not_found", iid.clone())),
        }
    } else {
        None
    };
    match store.resolve_machine(r) {
        Ok(m) => {
            let text = if fmt == "dot" {
                dot(&m.compiled, overlay.as_ref())
            } else {
                mermaid(&m.compiled, overlay.as_ref())
            };
            if let Some(path) = args.flags.get("o") {
                if let Err(e) = std::fs::write(path, &text) {
                    return emit_error(ctx, &ErrorObj::new("io/write", e.to_string()));
                }
            }
            emit_success(ctx, &Value::Str(text));
            0
        }
        Err(e) => emit_error(ctx, &e),
    }
}

pub static SPECS: &[CmdSpec] = &[CmdSpec {
    path: &["machine", "diagram"],
    positionals: &["machine"],
    flags: &["format", "instance", "o"],
    switches: &[],
    help: "Export a diagram",
    run: diagram,
}];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::Ctx;
    use crate::cli::machine;
    use std::collections::BTreeMap;

    #[test]
    fn write_file_and_unknown_format() {
        let dir = std::env::temp_dir().join(format!("fsm-dg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut c = Ctx::new(dir.clone(), false, false);
        let spec = format!(
            "{}/../fsm-core/tests/fixtures/machines/case_review.json",
            env!("CARGO_MANIFEST_DIR")
        );
        (machine::SPECS[0].run)(
            &mut c,
            &Args {
                positionals: vec![spec],
                flags: BTreeMap::new(),
                switches: Default::default(),
            },
        );
        let out = dir.join("d.mmd");
        assert_eq!(
            diagram(
                &mut c,
                &Args {
                    positionals: vec!["case_review".into()],
                    flags: BTreeMap::from([
                        ("format", "mermaid".into()),
                        ("o", out.display().to_string())
                    ]),
                    switches: Default::default()
                }
            ),
            0
        );
        assert!(out.exists());
        assert_eq!(
            diagram(
                &mut c,
                &Args {
                    positionals: vec!["case_review".into()],
                    flags: BTreeMap::from([("format", "png".into())]),
                    switches: Default::default()
                }
            ),
            2
        );
        assert_eq!(
            diagram(
                &mut c,
                &Args {
                    positionals: vec!["case_review".into()],
                    flags: BTreeMap::from([("instance", "missing".into())]),
                    switches: Default::default()
                }
            ),
            3
        );
    }
}
