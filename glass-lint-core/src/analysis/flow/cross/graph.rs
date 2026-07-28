use std::collections::BTreeMap;

use glass_lint_datastructures::{NamePath, NameTable};

use crate::{
    analysis::{
        ProjectSemanticModel, facts::FactId, project::state::LinkingSession, value::FunctionId,
    },
    api::compiler::{CompiledObjectFlow, CompiledObjectRequirement},
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

/// Pre-resolved requirement and sink member paths for one (flow, names) pair.
/// Built once and reused across all contexts for the same flow and module.
pub(super) struct FlowPathPlan {
    pub(super) req_members: Vec<Option<NamePath>>,
    pub(super) sink_members: Vec<Vec<NamePath>>,
}

impl FlowPathPlan {
    pub(super) fn build(flow: &CompiledObjectFlow, names: &NameTable) -> Self {
        let req_members = flow
            .requirements
            .iter()
            .map(|req| match req {
                CompiledObjectRequirement::MemberCall { member, .. } => names.lookup_path(member),
                CompiledObjectRequirement::PropertyWrite { .. } => None,
            })
            .collect();
        let sink_members = flow
            .sinks
            .iter()
            .map(|sink| {
                sink.member_calls
                    .iter()
                    .filter_map(|mc| names.lookup_path(mc))
                    .collect()
            })
            .collect();
        Self {
            req_members,
            sink_members,
        }
    }
}
