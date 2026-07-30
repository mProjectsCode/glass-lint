use std::collections::BTreeMap;

use crate::{
    analysis::{
        ProjectSemanticModel, facts::FactId, flow::planning::BoundFlowPaths,
        project::state::LinkingSession, value::FunctionId,
    },
    project::ModuleId,
};

/// Pre-computed qualified call targets keyed by (caller_module,
/// call_event_fact). Populated once and reused across all cross-flow phases.
pub(super) struct QualifiedCallGraph {
    targets: BTreeMap<(ModuleId, FactId), (ModuleId, FunctionId)>,
}

impl QualifiedCallGraph {
    pub(super) fn build(project: &ProjectSemanticModel, session: &mut LinkingSession) -> Self {
        let mut targets = BTreeMap::new();
        for module in project.modules() {
            let module_id = module.id();
            let stream = module.local().facts().stream();
            for effect in module.local().effects().iter_effects() {
                if effect.is_invalid() {
                    continue;
                }
                for call in effect.calls() {
                    let cref = call.as_ref(stream);
                    let Some(provenance) = cref.provenance() else {
                        continue;
                    };
                    if let Some(target) = project.qualified_function_target(
                        module_id,
                        cref.target(),
                        provenance,
                        session,
                    ) {
                        targets.insert((module_id, call.event()), target);
                    }
                }
            }
        }
        Self { targets }
    }

    pub(super) fn get(&self, module: ModuleId, event: FactId) -> Option<(ModuleId, FunctionId)> {
        self.targets.get(&(module, event)).copied()
    }
}

pub(super) type FlowPathPlan = BoundFlowPaths;
