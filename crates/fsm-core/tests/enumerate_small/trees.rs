use super::*;

#[derive(Clone, Debug)]
pub(super) struct Node {
    pub(super) kids: Vec<Node>,
}

pub(super) fn trees(n: usize, max_depth: u32) -> Vec<Node> {
    if n == 0 || max_depth == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![Node { kids: vec![] }];
    }
    forests(n - 1, max_depth - 1)
        .into_iter()
        .map(|kids| Node { kids })
        .collect()
}

pub(super) fn forests(n: usize, max_depth: u32) -> Vec<Vec<Node>> {
    if n == 0 {
        return vec![vec![]];
    }
    if max_depth == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for k in 1..=n {
        for tree in trees(k, max_depth) {
            for rest in forests(n - k, max_depth) {
                let mut forest = vec![tree.clone()];
                forest.extend(rest);
                out.push(forest);
            }
        }
    }
    out
}

#[derive(Clone, Debug)]
pub(super) struct Named {
    pub(super) name: String,
    pub(super) kids: Vec<Named>,
}

pub(super) fn name_forest(forest: &[Node]) -> Vec<Named> {
    let mut i = 0u32;
    fn walk(node: &Node, i: &mut u32) -> Named {
        let name = format!("s{i}");
        *i += 1;
        Named {
            name,
            kids: node.kids.iter().map(|child| walk(child, i)).collect(),
        }
    }
    forest.iter().map(|node| walk(node, &mut i)).collect()
}

pub(super) fn find_named<'a>(nodes: &'a [Named], name: &str) -> Option<&'a Named> {
    for node in nodes {
        if node.name == name {
            return Some(node);
        }
        if let Some(found) = find_named(&node.kids, name) {
            return Some(found);
        }
    }
    None
}

pub(super) fn collect_names(nodes: &[Named], out: &mut Vec<String>) {
    for node in nodes {
        out.push(node.name.clone());
        collect_names(&node.kids, out);
    }
}

pub(super) fn collect_compounds(nodes: &[Named], out: &mut Vec<String>) {
    for node in nodes {
        if !node.kids.is_empty() {
            out.push(node.name.clone());
        }
        collect_compounds(&node.kids, out);
    }
}

pub(super) fn collect_leaves(nodes: &[Named], out: &mut Vec<String>) {
    for node in nodes {
        if node.kids.is_empty() {
            out.push(node.name.clone());
        } else {
            collect_leaves(&node.kids, out);
        }
    }
}

fn contains_name(node: &Named, name: &str) -> bool {
    node.name == name || node.kids.iter().any(|child| contains_name(child, name))
}

pub(super) fn is_descendant_or_self(nodes: &[Named], owner: &str, name: &str) -> bool {
    find_named(nodes, owner).is_some_and(|node| contains_name(node, name))
}

#[derive(Clone, Debug)]
pub(super) struct InitialChoice {
    pub(super) root: String,
    pub(super) children: BTreeMap<String, String>,
}

pub(super) fn initial_choices(nodes: &[Named]) -> Vec<InitialChoice> {
    fn axes(nodes: &[Named], out: &mut Vec<(String, Vec<String>)>) {
        for node in nodes {
            if !node.kids.is_empty() {
                out.push((
                    node.name.clone(),
                    node.kids.iter().map(|child| child.name.clone()).collect(),
                ));
            }
            axes(&node.kids, out);
        }
    }

    let mut choices: Vec<InitialChoice> = nodes
        .iter()
        .map(|node| InitialChoice {
            root: node.name.clone(),
            children: BTreeMap::new(),
        })
        .collect();
    let mut all_axes = Vec::new();
    axes(nodes, &mut all_axes);
    for (owner, children) in all_axes {
        let mut next = Vec::new();
        for choice in choices {
            for child in &children {
                let mut expanded = choice.clone();
                expanded.children.insert(owner.clone(), child.clone());
                next.push(expanded);
            }
        }
        choices = next;
    }
    choices
}

pub(super) fn active_chain(nodes: &[Named], choice: &InitialChoice) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = choice.root.as_str();
    loop {
        out.push(current.to_string());
        let node = find_named(nodes, current).expect("initial choice names a state");
        if node.kids.is_empty() {
            break;
        }
        current = choice
            .children
            .get(current)
            .expect("compound initial choice exists");
    }
    out
}

pub(super) fn choice_activating(
    nodes: &[Named],
    choices: &[InitialChoice],
    state: &str,
) -> InitialChoice {
    choices
        .iter()
        .find(|choice| active_chain(nodes, choice).iter().any(|name| name == state))
        .unwrap_or_else(|| panic!("no initial choice activates {state}"))
        .clone()
}
