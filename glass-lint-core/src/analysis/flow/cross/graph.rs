use std::collections::BTreeMap;

use crate::analysis::{
    ProjectSemanticModel, QualifiedFunctionId, project::state::ExportLookupCache,
    trace::QualifiedEvent,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct QualifiedCallSite {
    event: QualifiedEvent,
}

/// Pre-computed qualified call targets keyed by a caller module and event.
/// Populated once and reused across all cross-flow phases.
pub(super) struct QualifiedCallGraph {
    targets: BTreeMap<QualifiedCallSite, QualifiedFunctionId>,
}

impl QualifiedCallGraph {
    pub(super) fn build(project: &ProjectSemanticModel, session: &mut ExportLookupCache) -> Self {
        let mut targets = BTreeMap::new();
        for module in project.modules() {
            let module_id = module.id();
            let stream = module.local().facts().stream();
            for effect in module.local().effects().iter_effects() {
                if effect.is_invalid() {
                    continue;
                }
                for call in effect.calls() {
                    let cref = stream.call_effect(call.event());
                    let Some(shape) = cref.shape() else {
                        continue;
                    };
                    let provenance = shape.provenance();
                    if let Some(target) = project.qualified_function_target(
                        module_id,
                        shape.target(),
                        provenance,
                        session,
                    ) {
                        targets.insert(
                            QualifiedCallSite {
                                event: QualifiedEvent::new(module_id, call.event()),
                            },
                            target,
                        );
                    }
                }
            }
        }
        Self { targets }
    }

    pub(super) fn get(&self, event: QualifiedEvent) -> Option<QualifiedFunctionId> {
        self.targets.get(&QualifiedCallSite { event }).copied()
    }
}
