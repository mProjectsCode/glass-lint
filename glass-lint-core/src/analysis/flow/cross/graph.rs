use std::collections::BTreeMap;

use crate::{
    analysis::{
        ProjectSemanticModel, facts::FactId, flow::planning::BoundFlowPaths,
        project::state::LinkingSession, value::FunctionId,
    },
    project::ModuleId,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct QualifiedCallSite {
    module: ModuleId,
    event: FactId,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct QualifiedCallTarget {
    module: ModuleId,
    function: FunctionId,
}

/// Pre-computed qualified call targets keyed by a caller module and event.
/// Populated once and reused across all cross-flow phases.
pub(super) struct QualifiedCallGraph {
    targets: BTreeMap<QualifiedCallSite, QualifiedCallTarget>,
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
                        targets.insert(
                            QualifiedCallSite {
                                module: module_id,
                                event: call.event(),
                            },
                            QualifiedCallTarget {
                                module: target.0,
                                function: target.1,
                            },
                        );
                    }
                }
            }
        }
        Self { targets }
    }

    pub(super) fn get(&self, module: ModuleId, event: FactId) -> Option<(ModuleId, FunctionId)> {
        self.targets
            .get(&QualifiedCallSite { module, event })
            .map(|target| (target.module, target.function))
    }
}

pub(super) type FlowPathPlan = BoundFlowPaths;
