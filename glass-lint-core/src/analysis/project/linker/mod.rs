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

use super::resolver::ExportResolver;
use crate::{
    analysis::{
        LinkedModuleTarget, ModuleId, ProjectModule, QualifiedRequestId,
        project::state::{ExportTable, LinkingSession, NormalizedModuleGraph, SccPartition},
        semantic::status::AnalysisStatus,
    },
    project::AnalysisDiagnostic,
};

enum SccPartitionState {
    Pending,
    Ready(SccPartition),
    Rejected,
}

impl SccPartitionState {
    fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }
}

// ---------------------------------------------------------------------------
// ProjectLinker
// ---------------------------------------------------------------------------

/// Transient linker that owns the module graph, SCC partition, mutable export
/// table, budgets, diagnostics, modules, and resolutions. Consumed into a
/// [`ProjectSemanticModel`](super::model::ProjectSemanticModel).
pub(super) struct ProjectLinker {
    modules: BTreeMap<ModuleId, ProjectModule>,
    resolutions: BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
    graph: Option<NormalizedModuleGraph>,
    scc_partition: SccPartitionState,
    exports: ExportTable,
    lookup_session: LinkingSession,
    link_budget: BudgetTracker,
    link_limit: usize,
    link_cycle_rounds: usize,
    diagnostics: Vec<AnalysisDiagnostic>,
    status: AnalysisStatus,
}

impl ProjectLinker {
    pub(super) fn with_export_resolver<T>(
        &mut self,
        operation: impl FnOnce(&mut ExportResolver<'_>) -> T,
    ) -> T {
        operation(&mut ExportResolver::from_maps(
            &self.modules,
            &self.resolutions,
            &self.exports,
            &mut self.lookup_session.lookup_cache,
        ))
    }

    /// Build a linker from pre-validated modules and resolutions.
    pub(super) fn new(
        modules: BTreeMap<ModuleId, ProjectModule>,
        resolutions: BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
        link_limit: usize,
        export_lookup_capacity: usize,
    ) -> Self {
        Self {
            modules,
            resolutions,
            graph: None,
            scc_partition: SccPartitionState::Pending,
            exports: ExportTable::default(),
            lookup_session: LinkingSession::new(export_lookup_capacity),
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
        let Self {
            modules, status, ..
        } = self;
        for module in modules.values() {
            let file_status = module.local().status().materialize_file(module.path());
            let path = module.path().clone();
            let unknown = module.local().interface().is_unknown();
            status.extend(&file_status);
            if unknown {
                status.record(
                    crate::analysis::semantic::status::StatusScope::File(path),
                    crate::analysis::semantic::status::IncompleteReason::UnsupportedModuleInterface,
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Graph construction and SCC-DAG export resolution
    // -----------------------------------------------------------------------

    pub(super) fn collect_graph_edges(&mut self) {
        let result = graph::GraphBuild::build(&self.modules, &self.resolutions, self.link_limit);
        self.status.extend(&result.status);
        if result.exhausted {
            self.link_budget.mark_exhausted();
        }
        self.graph = Some(result.graph);
        self.scc_partition = result
            .scc_partition
            .map_or(SccPartitionState::Rejected, SccPartitionState::Ready);
    }

    /// Build edges, resolve exports via SCC-DAG topological walk, validate
    /// imports, and canonicalize diagnostics.
    pub(super) fn build_graph_and_exports(&mut self) {
        self.collect_graph_edges();
        if self.scc_partition.is_ready() {
            self.resolve_export_table();
            self.validate_imported_exports();
        }
        self.diagnostics
            .sort_by(|left, right| left.ordering_key().cmp(&right.ordering_key()));
        self.diagnostics.dedup();
    }

    /// Consume the linker and construct the final semantic model.
    pub(super) fn finish(
        self,
        limits: &crate::AnalysisLimits,
    ) -> super::model::ProjectSemanticModel {
        let edge_count = self
            .graph
            .as_ref()
            .map_or(0, NormalizedModuleGraph::edge_count);
        super::model::ProjectSemanticModel::from_linker(
            super::model::LinkedProjectState {
                modules: self.modules,
                resolutions: self.resolutions,
                exports: self.exports,
                edge_count,
                link_cycle_rounds: self.link_cycle_rounds,
                diagnostics: self.diagnostics,
                status: self.status,
            },
            limits,
        )
    }
}
