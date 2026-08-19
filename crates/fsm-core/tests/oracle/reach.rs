use super::eval::is_compound;
use super::*;

pub fn brute_enterable(m: &CompiledMachine) -> std::collections::BTreeSet<String> {
    let (nodes, initial) = sequential_topology(&m.spec);
    let mut out = std::collections::BTreeSet::new();
    fn add_initial_chain(
        nodes: &[StateNode],
        name: &str,
        out: &mut std::collections::BTreeSet<String>,
    ) {
        out.insert(name.to_string());
        for child in initial_descent(nodes, name) {
            out.insert(child);
        }
    }

    // Creation enters the selected root and follows every declared initial.
    // History pseudostates are never configurations and therefore never enterable.
    add_initial_chain(nodes, initial, &mut out);
    let mut changed = true;
    while changed {
        changed = false;
        for tr in &m.spec.transitions {
            if !out.contains(&tr.from) {
                continue;
            }
            let Some(to) = &tr.to else { continue };
            let before = out.len();
            let target = find(nodes, to).expect("compiled transition target exists");
            if target.history.is_some() {
                // The reachability lemma models a history target as its owner's
                // initial configuration. A binding cannot introduce a state that
                // was not already active on an earlier path.
                let owner = parent_of(nodes, to).expect("history has an owner");
                let dom = lca(nodes, &tr.from, &owner);
                for name in entry_path(nodes, &dom, &owner) {
                    out.insert(name);
                }
                add_initial_chain(nodes, &owner, &mut out);
            } else {
                let dom = lca(nodes, &tr.from, to);
                for name in entry_path(nodes, &dom, to) {
                    out.insert(name);
                }
                if is_compound(target) {
                    for name in initial_descent(nodes, to) {
                        out.insert(name);
                    }
                }
            }
            if out.len() != before {
                changed = true;
            }
        }
    }
    out
}
