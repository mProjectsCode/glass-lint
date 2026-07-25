//! Module graph edge construction and SCC decomposition.

use std::collections::BTreeMap;

use glass_lint_datastructures::Budget;

use super::scc::{build_scc_dag_and_order, strongly_connected_components};
use crate::{
    analysis::{
        LinkedModuleTarget,
        lowering::status::{IncompleteReason, ResolutionKind, StatusScope},
        project::{model::MAX_SCC_SIZE, state::SccPartition},
    },
    project::is_internal_module_request as is_internal_request,
};

impl super::ProjectLinker {
    /// Convert internal resolution records into bounded graph edges, compute
    /// SCCs, build the SCC DAG, and compute the topological order.
    pub(super) fn collect_graph_edges(&mut self) {
        let mut edge_budget = Budget::new(self.link_limit);
        for module in self.modules.values() {
            self.graph.ensure_node(module.id());
            for request in module.local().interface().requests() {
                let Some(request_id) = self.request_id(module.id(), request) else {
                    continue;
                };
                let Some(resolution) = self.resolutions.get(&request_id) else {
                    if is_internal_request(request.specifier()) {
                        self.status.record(
                            StatusScope::File(module.path().clone()),
                            IncompleteReason::MissingInternalResolution {
                                request: request.specifier().to_string(),
                            },
                        );
                    }
                    continue;
                };
                if let LinkedModuleTarget::Internal { id, .. } = resolution {
                    if edge_budget.try_push() {
                        self.graph.insert_edge(module.id(), *id);
                    } else {
                        self.link_budget.mark_exhausted();
                    }
                } else if matches!(resolution, LinkedModuleTarget::Missing)
                    && is_internal_request(request.specifier())
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
        self.graph.normalize();

        let components =
            strongly_connected_components(self.graph.forward(), self.modules.keys().copied());

        let oversized = components.iter().any(|c| c.len() > MAX_SCC_SIZE);
        if oversized {
            self.link_budget.mark_exhausted();
            self.scc_partition = SccPartition {
                components,
                dag: BTreeMap::new(),
                order: Vec::new(),
            };
            return;
        }

        let (dag, order) = build_scc_dag_and_order(self.graph.forward(), &components);
        self.scc_partition = SccPartition {
            components,
            dag,
            order,
        };
    }
}
