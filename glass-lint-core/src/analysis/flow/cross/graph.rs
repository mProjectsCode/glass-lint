use std::collections::BTreeMap;

use crate::{
    analysis::{
        ProjectModule, ProjectSemanticModel, QualifiedFunctionId,
        facts::{FactStream, Frozen},
        flow::effect::{EffectCall, FunctionEffect},
        project::state::ExportLookupCache,
        trace::QualifiedEvent,
    },
    project::ModuleId,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct QualifiedCallSite {
    event: QualifiedEvent,
}

/// Visit every call in valid function effects in deterministic module/effect
/// order. Consumers resolve only the call details they need.
pub(super) fn for_each_valid_call(
    project: &ProjectSemanticModel,
    mut visit: impl FnMut(ModuleId, &FunctionEffect, &EffectCall, &FactStream<Frozen>),
) {
    for module in project.modules() {
        for_each_valid_call_in_module(module, &mut visit);
    }
}

pub(super) fn for_each_valid_call_in_module(
    module: &ProjectModule,
    mut visit: impl FnMut(ModuleId, &FunctionEffect, &EffectCall, &FactStream<Frozen>),
) {
    let module_id = module.id();
    let stream = module.local().facts().stream();
    for effect in module.local().effects().iter_effects() {
        if effect.is_invalid() {
            continue;
        }
        for call in effect.calls() {
            visit(module_id, effect, call, stream);
        }
    }
}

/// Pre-computed qualified call targets keyed by a caller module and event.
/// Populated once and reused across all cross-flow phases.
pub(super) struct QualifiedCallGraph {
    targets: BTreeMap<QualifiedCallSite, QualifiedFunctionId>,
}

impl QualifiedCallGraph {
    pub(super) fn build(project: &ProjectSemanticModel, session: &mut ExportLookupCache) -> Self {
        let mut targets = BTreeMap::new();
        for_each_valid_call(project, |module_id, _, call, stream| {
            let Some(shape) = stream.call_shape(call.event()) else {
                return;
            };
            let provenance = shape.provenance();
            if let Some(target) =
                project.qualified_function_target(module_id, shape.target(), provenance, session)
            {
                targets.insert(
                    QualifiedCallSite {
                        event: QualifiedEvent::new(module_id, call.event()),
                    },
                    target,
                );
            }
        });
        Self { targets }
    }

    pub(super) fn get(&self, event: QualifiedEvent) -> Option<QualifiedFunctionId> {
        self.targets.get(&QualifiedCallSite { event }).copied()
    }
}
