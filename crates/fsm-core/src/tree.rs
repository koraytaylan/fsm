//! Hierarchy tables: parent, depth, LCA, exit/entry, descents.

#![allow(clippy::collapsible_if)]

use std::collections::BTreeMap;

use crate::spec::{HistoryKind, StateNode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Leaf,
    Compound,
    History(HistoryKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    pub names: Vec<String>,
    pub parent: Vec<Option<u16>>,
    pub depth: Vec<u8>,
    pub children: Vec<Vec<u16>>,
    pub initial_child: Vec<Option<u16>>,
    pub kind: Vec<NodeKind>,
    pub index: BTreeMap<String, u16>,
}

impl Tree {
    pub fn build(states: &[StateNode]) -> Tree {
        let mut names = Vec::new();
        let mut parent = Vec::new();
        let mut depth = Vec::new();
        let mut kind = Vec::new();
        let mut children: Vec<Vec<u16>> = Vec::new();
        let mut index = BTreeMap::new();
        let mut stack: Vec<(&StateNode, Option<u16>)> = Vec::new();
        for child in states.iter().rev() {
            stack.push((child, None));
        }
        while let Some((node, par)) = stack.pop() {
            let idx = names.len() as u16;
            names.push(node.name.clone());
            parent.push(par);
            let d = match par {
                None => 1,
                Some(p) => depth[p as usize] + 1,
            };
            depth.push(d);
            let k = if node.history.is_some() {
                NodeKind::History(node.history.unwrap())
            } else if node.states.is_empty() {
                NodeKind::Leaf
            } else {
                NodeKind::Compound
            };
            kind.push(k);
            children.push(Vec::new());
            index.insert(node.name.clone(), idx);
            if let Some(p) = par {
                children[p as usize].push(idx);
            }
            for child in node.states.iter().rev() {
                stack.push((child, Some(idx)));
            }
        }
        let mut initial_child = vec![None; names.len()];
        fn fill_initial(
            nodes: &[StateNode],
            index: &BTreeMap<String, u16>,
            initial_child: &mut [Option<u16>],
        ) {
            for n in nodes {
                if let Some(init) = &n.initial {
                    if let (Some(&me), Some(&ch)) = (index.get(&n.name), index.get(init)) {
                        initial_child[me as usize] = Some(ch);
                    }
                }
                fill_initial(&n.states, index, initial_child);
            }
        }
        fill_initial(states, &index, &mut initial_child);
        Tree {
            names,
            parent,
            depth,
            children,
            initial_child,
            kind,
            index,
        }
    }

    pub fn id(&self, name: &str) -> Option<u16> {
        self.index.get(name).copied()
    }

    pub fn chain(&self, leaf: u16) -> Vec<u16> {
        let mut out = Vec::new();
        let mut cur = Some(leaf);
        while let Some(i) = cur {
            out.push(i);
            cur = self.parent[i as usize];
        }
        out
    }

    pub fn proper_lca(&self, a: u16, b: u16) -> Option<u16> {
        let mut x = self.parent[a as usize];
        let mut y = self.parent[b as usize];
        while depth_of(x, &self.depth) > depth_of(y, &self.depth) {
            x = x.and_then(|i| self.parent[i as usize]);
        }
        while depth_of(y, &self.depth) > depth_of(x, &self.depth) {
            y = y.and_then(|i| self.parent[i as usize]);
        }
        while x != y {
            x = x.and_then(|i| self.parent[i as usize]);
            y = y.and_then(|i| self.parent[i as usize]);
        }
        x
    }

    pub fn exit_set(&self, leaf: u16, dom: Option<u16>) -> Vec<u16> {
        let mut out = Vec::new();
        let mut cur = Some(leaf);
        while let Some(i) = cur {
            if Some(i) == dom {
                break;
            }
            out.push(i);
            cur = self.parent[i as usize];
        }
        out
    }

    pub fn entry_path(&self, dom: Option<u16>, target: u16) -> Vec<u16> {
        let mut walk = Vec::new();
        let mut cur = Some(target);
        while let Some(i) = cur {
            if Some(i) == dom {
                break;
            }
            walk.push(i);
            cur = self.parent[i as usize];
        }
        walk.reverse();
        walk
    }

    pub fn initial_descent(&self, from: u16) -> Vec<u16> {
        let mut out = Vec::new();
        let mut cur = self.initial_child[from as usize];
        while let Some(i) = cur {
            out.push(i);
            cur = self.initial_child[i as usize];
        }
        out
    }

    /// Dotted display path from the nearest compound ancestor, e.g. `in_review.docs_review`.
    pub fn dotted_path(&self, leaf: &str) -> String {
        let Some(id) = self.id(leaf) else {
            return leaf.to_string();
        };
        let mut names = Vec::new();
        let mut cur = Some(id);
        while let Some(i) = cur {
            names.push(self.names[i as usize].clone());
            cur = self.parent[i as usize];
        }
        names.reverse();
        names.join(".")
    }

    /// Active configuration: ancestors then leaf, root-first, excluding history nodes.
    pub fn configuration(&self, leaf: &str) -> Vec<String> {
        let Some(id) = self.id(leaf) else {
            return vec![leaf.to_string()];
        };
        let mut names = Vec::new();
        let mut cur = Some(id);
        while let Some(i) = cur {
            if !matches!(self.kind[i as usize], NodeKind::History(_)) {
                names.push(self.names[i as usize].clone());
            }
            cur = self.parent[i as usize];
        }
        names.reverse();
        names
    }

    pub fn history_owner(&self, hist: u16) -> Option<u16> {
        self.parent[hist as usize]
    }

    pub fn history_descent(&self, hist: u16, binding: Option<&str>) -> Vec<u16> {
        let owner = match self.history_owner(hist) {
            Some(o) => o,
            None => return Vec::new(),
        };
        let kind = match &self.kind[hist as usize] {
            NodeKind::History(k) => *k,
            _ => return self.initial_descent(owner),
        };
        match (kind, binding) {
            (_, None) => self.initial_descent(owner),
            (crate::spec::HistoryKind::Deep, Some(name)) => {
                if let Some(leaf) = self.id(name) {
                    // path from just below owner down to leaf
                    self.entry_path(Some(owner), leaf)
                } else {
                    self.initial_descent(owner)
                }
            }
            (crate::spec::HistoryKind::Shallow, Some(name)) => {
                if let Some(child) = self.id(name) {
                    let mut out = vec![child];
                    out.extend(self.initial_descent(child));
                    out
                } else {
                    self.initial_descent(owner)
                }
            }
        }
    }
}

fn depth_of(x: Option<u16>, depth: &[u8]) -> u8 {
    x.map(|i| depth[i as usize]).unwrap_or(0)
}
