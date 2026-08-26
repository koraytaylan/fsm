//! The structural event-handling matrix.

use std::collections::BTreeMap;

use crate::machine::CompiledMachine;
use crate::tree::{NodeKind, Tree};

use super::find_machine_node;

/// Report structural event handling for every real leaf.
///
/// Terminal leaves are inert: their ancestor handlers are not candidates, even
/// while another region keeps a parallel instance running.
pub fn completeness_matrix(m: &CompiledMachine, t: &Tree) -> BTreeMap<(String, String), String> {
    let policy = match m.spec.on_unhandled {
        crate::spec::Unhandled::Ignore => "ignore",
        crate::spec::Unhandled::Reject => "reject",
    };
    let mut out = BTreeMap::new();
    let leaves: Vec<u16> = t
        .kind
        .iter()
        .enumerate()
        .filter_map(|(i, k)| match k {
            NodeKind::Leaf => Some(i as u16),
            _ => None,
        })
        .collect();
    for leaf in leaves {
        let chain = t.chain(leaf);
        let lname = t.names[leaf as usize].clone();
        let terminal = find_machine_node(&m.spec, &lname).is_some_and(|node| node.terminal);
        for ev in &m.spec.events {
            let mut cell = format!("unhandled({policy})");
            if !terminal {
                for &sid in &chain {
                    let sname = &t.names[sid as usize];
                    if m.transitions_by
                        .contains_key(&(sname.clone(), ev.name.clone()))
                    {
                        cell = format!("handled@{sname}");
                        break;
                    }
                }
            }
            out.insert((lname.clone(), ev.name.clone()), cell);
        }
    }
    out
}
