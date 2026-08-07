//! Module graph edge construction and SCC-DAG preparation.

use glass_lint_datastructures::Budget;

use crate::{
    analysis::{
        LinkedModuleTarget,
        lowering::status::{IncompleteReason, ResolutionKind, StatusScope},
        project::{linker::ProjectLinker, model::MAX_SCC_SIZE, state::SccPartition},
    },
    project::is_internal_module_request,
};

impl ProjectLinker {
    /// Convert internal resolution records into bounded graph edges, compute
    /// SCCs, build the SCC DAG, and compute the topological order.
    pub(super) fn collect_graph_edges(&mut self) {
        let mut graph = super::super::state::ModuleGraph::default();
        let mut edge_budget = Budget::new(self.link_limit);
        for module in self.modules.values() {
            graph.ensure_node(module.id());
            for request in module.local().interface().requests() {
                let Some(request_id) = self.request_id(module.id(), request) else {
                    continue;
                };
                let Some(resolution) = self.resolutions.get(&request_id) else {
                    if is_internal_module_request(request.specifier()) {
                        self.status.record(
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
                    } else {
                        self.link_budget.mark_exhausted();
                    }
                } else if matches!(resolution, LinkedModuleTarget::Missing)
                    && is_internal_module_request(request.specifier())
                {
                    self.status.record(
                        StatusScope::File(module.path().clone()),
                        IncompleteReason::MissingInternalResolution {
                            request: request.specifier().to_string(),
                        },
                    );
                } else if matches!(resolution, LinkedModuleTarget::OutsideProject { .. }) {
                    self.status.record(
                        StatusScope::File(module.path().clone()),
                        IncompleteReason::UnsupportedResolution {
                            request: request.specifier().to_string(),
                            kind: ResolutionKind::OutsideProject,
                        },
                    );
                } else if matches!(resolution, LinkedModuleTarget::Unsupported { .. }) {
                    self.status.record(
                        StatusScope::File(module.path().clone()),
                        IncompleteReason::UnsupportedResolution {
                            request: request.specifier().to_string(),
                            kind: ResolutionKind::Unsupported,
                        },
                    );
                }
            }
        }
        if edge_budget.exhausted() {
            self.link_budget.mark_exhausted();
        }
        let graph = graph.normalize();

        if let Some(partition) = graph.scc_partition(MAX_SCC_SIZE) {
            self.scc_partition = partition;
        } else {
            self.link_budget.mark_exhausted();
            self.scc_partition = SccPartition::default();
        }
        self.graph = Some(graph);
    }
}
