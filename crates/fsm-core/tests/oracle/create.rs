use super::eval::{apply_entry_chain, parse_init};
use super::step::NaiveMicro;
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
    // SPEC Appendix B: the standard budget for each of the trigger, the
    // ceiling's worth of reactions, and the closing scan.
    let mut budget = Budget::new(4096 * (64 + 2));
    for c in &m.spec.context {
        let v = if let Some(ov) = overrides.get(&c.name) {
            ov.clone()
        } else {
            parse_init(&c.init, &c.ty)?
        };
        ctx.insert(c.name.clone(), v);
    }
    let mut effects = Vec::new();
    let mut raised = Vec::new();
    let mut signalled = Vec::new();
    let mut entered = Vec::new();
    let configuration_after = match &m.spec.topology {
        Topology::Sequential { states, initial } => {
            let path = apply_entry_chain(
                states,
                initial,
                &mut ctx,
                &mut budget,
                &mut effects,
                &mut raised,
                &mut signalled,
            )?;
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
                    &mut raised,
                    &mut signalled,
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
    // Creation runs a macrostep like everything else: the initial entry is
    // its trigger, and the state it starts from is the configuration it
    // entered, so no region can have "become" terminal.
    let before = InstanceState {
        status: Status::Running,
        configuration: configuration_after.clone(),
        ctx: BTreeMap::new(),
        history: BTreeMap::new(),
        deadlines: BTreeMap::new(),
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    let first = NaiveMicro {
        configuration_after,
        ctx,
        history_after: BTreeMap::new(),
        internal: false,
        region: None,
        source: String::new(),
        transition_index: 0,
        exited: Vec::new(),
        entered,
        raised,
        signalled,
    };
    super::macrostep::run_reactions(m, &before, first, effects, now_ms, &mut budget)
}
