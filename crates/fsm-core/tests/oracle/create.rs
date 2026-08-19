use super::deadline::{clear_terminal_region_deadlines, update_deadline_schedules};
use super::eval::{apply_entry_chain, eval_invariants, parse_init};
use super::*;

pub fn naive_create(
    m: &CompiledMachine,
    overrides: &BTreeMap<String, Val>,
) -> Result<Applied, Rejection> {
    naive_create_at(m, overrides, 0)
}

pub fn naive_create_at(
    m: &CompiledMachine,
    overrides: &BTreeMap<String, Val>,
    now_ms: i64,
) -> Result<Applied, Rejection> {
    let mut ctx = BTreeMap::new();
    let mut budget = Budget::new(4096);
    for c in &m.spec.context {
        let v = if let Some(ov) = overrides.get(&c.name) {
            ov.clone()
        } else {
            parse_init(&c.init, &c.ty)?
        };
        ctx.insert(c.name.clone(), v);
    }
    let mut effects = Vec::new();
    let mut entered = Vec::new();
    let configuration_after = match &m.spec.topology {
        Topology::Sequential { states, initial } => {
            let path = apply_entry_chain(states, initial, &mut ctx, &mut budget, &mut effects)?;
            let leaf = path.last().cloned().unwrap_or_else(|| initial.to_string());
            entered.extend(path);
            ActiveConfiguration::Sequential { leaf }
        }
        Topology::Parallel { regions } => {
            let mut leaves = BTreeMap::new();
            for region in regions {
                let path = apply_entry_chain(
                    &region.states,
                    &region.initial,
                    &mut ctx,
                    &mut budget,
                    &mut effects,
                )?;
                let leaf = path
                    .last()
                    .cloned()
                    .unwrap_or_else(|| region.initial.clone());
                leaves.insert(region.name.clone(), leaf);
                entered.extend(path);
            }
            ActiveConfiguration::Parallel { leaves }
        }
    };
    let flags = eval_invariants(&m.spec, &ctx, &mut budget)?;
    let mut deadlines_after = update_deadline_schedules(
        &m.spec,
        &BTreeMap::new(),
        &[],
        &entered,
        &ctx,
        now_ms,
        &mut budget,
    )?;
    clear_terminal_region_deadlines(&m.spec, &configuration_after, &mut deadlines_after);
    let status_after = if configuration_is_terminal(&m.spec, &configuration_after) {
        deadlines_after.clear();
        Status::Completed
    } else {
        Status::Running
    };
    Ok(Applied {
        configuration_after,
        ctx_after: ctx,
        history_after: BTreeMap::new(),
        deadlines_after,
        effects,
        monitor_flags: flags,
        status_after,
        internal: false,
        region: None,
        source_state: String::new(),
        transition_idx: 0,
        exited: Vec::new(),
        entered,
        trace: Default::default(),
    })
}
