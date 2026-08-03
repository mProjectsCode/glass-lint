//! Transient linker state for graph construction, SCC-DAG export resolution,
//! and bounded budget enforcement. Consumed into a final
//! [`ProjectSemanticModel`](super::model::ProjectSemanticModel).
//!
//! Graph construction is the boundary between typed resolver answers and
//! core's linker. Only internal targets become edges; all other outcomes are
//! retained as diagnostics.

mod export;
mod graph;

use std::collections::BTreeMap;

use glass_lint_datastructures::BudgetTracker;

use crate::{
    analysis::{
        LinkedModuleTarget, ModuleId, ProjectModule, QualifiedRequestId,
        lowering::status::AnalysisStatus,
        module,
        project::state::{ExportTable, LinkingSession, ModuleGraph, SccPartition},
    },
    project::AnalysisDiagnostic,
};

// ---------------------------------------------------------------------------
// ProjectLinker
// ---------------------------------------------------------------------------

/// Transient linker that owns the module graph, SCC partition, mutable export
/// table, budgets, diagnostics, modules, and resolutions. Consumed into a
/// [`ProjectSemanticModel`](super::model::ProjectSemanticModel).
pub(super) struct ProjectLinker {
    pub(super) modules: BTreeMap<ModuleId, ProjectModule>,
    pub(super) resolutions: BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
    pub(super) graph: ModuleGraph,
    pub(super) scc_partition: SccPartition,
    pub(super) exports: ExportTable,
    pub(super) lookup_session: LinkingSession,
    pub(super) link_budget: BudgetTracker,
    pub(super) link_limit: usize,
    pub(super) link_cycle_rounds: usize,
    pub(super) diagnostics: Vec<AnalysisDiagnostic>,
    pub(super) status: AnalysisStatus,
}

impl ProjectLinker {
    /// Build a linker from pre-validated modules and resolutions.
    pub(super) fn new(
        modules: BTreeMap<ModuleId, ProjectModule>,
        resolutions: BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
        link_limit: usize,
    ) -> Self {
        Self {
            modules,
            resolutions,
            graph: ModuleGraph::default(),
            scc_partition: SccPartition::default(),
            exports: ExportTable::default(),
            lookup_session: LinkingSession::new(link_limit),
            link_cycle_rounds: 0,
            diagnostics: Vec::new(),
            status: AnalysisStatus::default(),
            link_budget: BudgetTracker::default(),
            link_limit,
        }
    }

    // -----------------------------------------------------------------------
    // Status propagation (runs before graph construction)
    // -----------------------------------------------------------------------

    pub(super) fn propagate_local_status(&mut self) {
        let ids: Vec<ModuleId> = self.modules.keys().copied().collect();
        for id in ids {
            let (file_status, path, unknown) = {
                let Some(module) = self.modules.get(&id) else {
                    continue;
                };
                (
                    module.local().status().for_file(module.path()),
                    module.path().clone(),
                    module.local().interface().is_unknown(),
                )
            };
            self.status.extend(&file_status);
            if unknown {
                self.status.record(
                    crate::analysis::lowering::status::StatusScope::File(path),
                    crate::analysis::lowering::status::IncompleteReason::UnsupportedModuleInterface {
                        kind: crate::analysis::lowering::status::ModuleInterfaceKind::CommonJsExports,
                    },
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Graph construction and SCC-DAG export resolution
    // -----------------------------------------------------------------------

    /// Build edges, resolve exports via SCC-DAG topological walk, validate
    /// imports, and canonicalize diagnostics.
    pub(super) fn build_graph_and_exports(&mut self) {
        self.collect_graph_edges();
        self.resolve_export_table();
        self.validate_imported_exports();
        self.diagnostics
            .sort_by(|left, right| left.ordering_key().cmp(&right.ordering_key()));
        self.diagnostics.dedup();
    }

    /// Consume the linker and construct the final semantic model.
    pub(super) fn finish(
        self,
        limits: &crate::AnalysisLimits,
    ) -> super::model::ProjectSemanticModel {
        let edge_count = self.graph.edge_count();
        super::model::ProjectSemanticModel {
            modules: self.modules,
            resolutions: self.resolutions,
            exports: self.exports,
            edge_count,
            link_cycle_rounds: self.link_cycle_rounds,
            diagnostics: self.diagnostics,
            status: self.status,
            flow_limit: limits.flow_operations(),
            effect_limit: limits.effect_operations(),
            trace_limit: limits.trace_nodes(),
            trace_arena: crate::analysis::trace::TraceArena::new(limits.trace_nodes()),
        }
    }

    /// Return the stable internal identity for one local request.
    pub(super) fn request_id(
        &self,
        module: ModuleId,
        request: &module::ModuleRequest,
    ) -> Option<QualifiedRequestId> {
        self.modules.get(&module)?;
        Some(QualifiedRequestId::new(module, request.id()))
    }
}
