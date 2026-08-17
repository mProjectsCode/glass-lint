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
        project::{
            resolver::{self, ExportResolver},
            state::{ExportLookupCache, ExportTable, NormalizedModuleGraph, SccPartition},
        },
        semantic::status::AnalysisStatus,
    },
    project::SourceLocation,
};

// ---------------------------------------------------------------------------
// ProjectLinker
// ---------------------------------------------------------------------------

/// Transient linker that owns the module graph, SCC partition, mutable export
/// table, budgets, and transient graph state. Retained modules, resolutions,
/// diagnostics, and status are held in the linked state consumed into a
/// [`ProjectSemanticModel`](super::model::ProjectSemanticModel).
pub(super) struct ProjectLinker {
    state: super::model::LinkedProjectState,
    graph: NormalizedModuleGraph,
    scc_partition: Option<SccPartition>,
    lookup_session: ExportLookupCache,
    link_budget: BudgetTracker,
    link_limit: usize,
}

impl ProjectLinker {
    pub(super) fn with_export_resolver<T>(
        &mut self,
        operation: impl FnOnce(&mut ExportResolver<'_>) -> T,
    ) -> T {
        resolver::with_export_resolver(
            &self.state.modules,
            &self.state.resolutions,
            &self.state.exports,
            &mut self.lookup_session,
            operation,
        )
    }

    /// Build a linker from pre-validated modules and resolutions.
    pub(super) fn new(
        modules: BTreeMap<ModuleId, ProjectModule>,
        resolutions: BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
        link_limit: usize,
        export_lookup_capacity: usize,
    ) -> Self {
        Self {
            state: super::model::LinkedProjectState {
                modules,
                resolutions,
                exports: ExportTable::default(),
                edge_count: 0,
                link_cycle_rounds: 0,
                diagnostics: Vec::new(),
                status: AnalysisStatus::default(),
            },
            graph: NormalizedModuleGraph::default(),
            scc_partition: None,
            lookup_session: ExportLookupCache::new(export_lookup_capacity),
            link_budget: BudgetTracker::default(),
            link_limit,
        }
    }

    // -----------------------------------------------------------------------
    // Status propagation (runs before graph construction)
    // -----------------------------------------------------------------------

    pub(super) fn propagate_local_status(&mut self) {
        let super::model::LinkedProjectState {
            modules, status, ..
        } = &mut self.state;
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
        let result = graph::GraphBuild::build(
            &self.state.modules,
            &self.state.resolutions,
            self.link_limit,
        );
        self.state.status.extend(&result.status);
        if result.exhausted {
            self.link_budget.mark_exhausted();
        }
        self.graph = result.graph;
        self.scc_partition = result.scc_partition;
    }

    /// Build edges, resolve exports via SCC-DAG topological walk, validate
    /// imports, and canonicalize diagnostics.
    pub(super) fn build_graph_and_exports(&mut self) {
        self.collect_graph_edges();
        if self.scc_partition.is_some() {
            self.resolve_export_table();
            self.validate_imported_exports();
        }
        self.state.diagnostics.sort_by(|left, right| {
            (
                left.location().map(SourceLocation::path),
                left.code().as_str(),
                left.message(),
            )
                .cmp(&(
                    right.location().map(SourceLocation::path),
                    right.code().as_str(),
                    right.message(),
                ))
        });
        self.state.diagnostics.dedup();
    }

    /// Consume the linker and construct the final semantic model.
    pub(super) fn finish(
        self,
        limits: &crate::AnalysisLimits,
    ) -> super::model::ProjectSemanticModel {
        let mut state = self.state;
        state.edge_count = self.graph.edge_count();
        super::model::ProjectSemanticModel::from_linker(state, limits)
    }
}
