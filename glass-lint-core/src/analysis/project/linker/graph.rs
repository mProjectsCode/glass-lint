//! Module graph edge construction and SCC-DAG preparation.

use std::collections::BTreeMap;

use glass_lint_datastructures::Budget;

use crate::{
    analysis::{
        LinkedModuleTarget, ModuleId, ProjectModule, QualifiedRequestId,
        lowering::status::{AnalysisStatus, IncompleteReason, ResolutionKind, StatusScope},
        project::{
            model::MAX_SCC_SIZE,
            state::{ModuleGraph, NormalizedModuleGraph, SccPartition},
        },
    },
    project::is_internal_module_request,
};

pub(super) struct GraphBuild {
    pub(super) graph: NormalizedModuleGraph,
    pub(super) scc_partition: SccPartition,
    pub(super) status: AnalysisStatus,
    pub(super) exhausted: bool,
}

impl GraphBuild {
    /// Convert internal resolution records into bounded graph edges, compute
    /// SCCs, build the SCC DAG, and compute the topological order.
    pub(super) fn build(
        modules: &BTreeMap<ModuleId, ProjectModule>,
        resolutions: &BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
        link_limit: usize,
    ) -> Self {
        let mut graph = ModuleGraph::default();
        let mut status = AnalysisStatus::default();
        let mut edge_budget = Budget::new(link_limit);
        for module in modules.values() {
            graph.ensure_node(module.id());
            for request in module.local().interface().requests() {
                let request_id = QualifiedRequestId::new(module.id(), request.id());
                let Some(resolution) = resolutions.get(&request_id) else {
                    if is_internal_module_request(request.specifier()) {
                        status.record(
                            StatusScope::File(module.path().clone()),
                            IncompleteReason::MissingInternalResolution {
                                request: request.specifier().to_string(),
                            },
                        );
                    }
                    continue;
                };
                if let LinkedModuleTarget::Internal { id } = resolution {
                    if edge_budget.try_push() {
                        graph.insert_edge(module.id(), *id);
                    }
                } else if matches!(resolution, LinkedModuleTarget::Missing)
                    && is_internal_module_request(request.specifier())
                {
                    status.record(
                        StatusScope::File(module.path().clone()),
                        IncompleteReason::MissingInternalResolution {
                            request: request.specifier().to_string(),
                        },
                    );
                } else if matches!(resolution, LinkedModuleTarget::OutsideProject { .. }) {
                    status.record(
                        StatusScope::File(module.path().clone()),
                        IncompleteReason::UnsupportedResolution {
                            request: request.specifier().to_string(),
                            kind: ResolutionKind::OutsideProject,
                        },
                    );
                } else if matches!(resolution, LinkedModuleTarget::Unsupported { .. }) {
                    status.record(
                        StatusScope::File(module.path().clone()),
                        IncompleteReason::UnsupportedResolution {
                            request: request.specifier().to_string(),
                            kind: ResolutionKind::Unsupported,
                        },
                    );
                }
            }
        }

        let mut exhausted = edge_budget.exhausted();
        let graph = graph.normalize();
        let scc_partition = graph.scc_partition(MAX_SCC_SIZE).unwrap_or_else(|| {
            exhausted = true;
            SccPartition::default()
        });
        Self {
            graph,
            scc_partition,
            status,
            exhausted,
        }
    }
}
