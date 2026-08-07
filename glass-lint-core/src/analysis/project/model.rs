//! The linked, partitioned semantic model for a project. Local value and fact
//! identities remain owned by their module; the overlay stores qualified
//! resolution results rather than merging lexical arenas.

use std::collections::BTreeMap;

use glass_lint_datastructures::NameTable;
use smol_str::SmolStr;

use crate::{
    analysis::{
        facts::{FactStream, Frozen, SemanticFact},
        flow::effect::FunctionEffect,
        local::{LocalArtifact, ProjectModule},
        lowering::status::{AnalysisStatus, IncompleteReason, StatusScope},
        model::{module::ModuleRequestId, scope::FunctionId, value::ValueId},
        project::{
            linker::ProjectLinker,
            projection::ProjectionOutcome,
            resolver::{ExportResolver, ProjectLookupView},
            state::{ExportTable, LinkingSession},
        },
        syntax::SymbolCallProvenance,
        trace::{QualifiedEvent, TraceArena, TraceNodeId, TraceStep},
    },
    api::{
        classification::{ClassificationResult, RuleIndex},
        compiler::{CompiledRuleRecord, CompiledRuleSelection},
    },
    project::{
        AnalysisDiagnostic, LinkedModuleTarget, ModuleId, ProjectPhaseError, ProjectRelativePath,
        ResolutionRequestKey, ResolutionTable, ResolverOutcome, SourceLocation,
    },
};

pub(super) const MAX_EXPORT_DEPTH: usize = 1024;
pub(super) const MAX_EXPORT_ENTRIES: usize = 1_000_000;
pub(super) const MAX_SCC_SIZE: usize = 4_096;
pub(super) const MAX_PROJECT_REQUESTS: usize = 500_000;

// ---------------------------------------------------------------------------
// QualifiedRequestId
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct QualifiedRequestId {
    module: ModuleId,
    request: ModuleRequestId,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis) struct QualifiedFunctionId {
    module: ModuleId,
    function: FunctionId,
}

impl QualifiedFunctionId {
    pub(in crate::analysis) const fn new(module: ModuleId, function: FunctionId) -> Self {
        Self { module, function }
    }

    pub(in crate::analysis) const fn module(self) -> ModuleId {
        self.module
    }

    pub(in crate::analysis) const fn function(self) -> FunctionId {
        self.function
    }
}

impl QualifiedRequestId {
    pub fn new(module: ModuleId, request: ModuleRequestId) -> Self {
        Self { module, request }
    }
}

// ---------------------------------------------------------------------------
// ExportResolution
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::analysis) enum ExportResolution {
    /// Identity resolved to an external module export.
    External { module: SmolStr, export: SmolStr },
    /// Identity resolved to a configured global.
    Global { name: SmolStr },
    /// Identity resolved to a static string.
    StaticString { value: String },
    /// Identity qualified to another project module.
    Qualified { module: ModuleId, export: SmolStr },
    /// Identity could not be established.
    Unknown,
    /// Multiple linked paths proved incompatible identities.
    Ambiguous,
}

impl ExportResolution {
    /// Convert to a call provenance when this identity maps to an external
    /// module export or a known global. Returns `None` for qualified,
    /// static-string, and unknown identities.
    pub(in crate::analysis) fn to_call_provenance(&self) -> Option<SymbolCallProvenance> {
        match self {
            Self::External { module, export } => Some(SymbolCallProvenance::ModuleExport {
                module: module.clone(),
                export: export.clone(),
            }),
            Self::Global { name } => Some(SymbolCallProvenance::Global { name: name.clone() }),
            Self::Qualified { .. }
            | Self::StaticString { .. }
            | Self::Ambiguous
            | Self::Unknown => None,
        }
    }

    /// Return the static string value when this identity is a `StaticString`.
    pub(in crate::analysis) fn static_string_value(&self) -> Option<&str> {
        match self {
            Self::StaticString { value } => Some(value.as_str()),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// ResolvedLinkInput
// ---------------------------------------------------------------------------

/// Combined linker input after local analysis and resolution validation.
/// Source text has been consumed or dropped; only module-level semantic
/// artifacts and their resolved import targets remain.
pub struct ResolvedLinkInput {
    modules: BTreeMap<ModuleId, ProjectModule>,
    resolutions: BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
}

impl ResolvedLinkInput {
    /// Assemble the linker input from validated module ownership. Consumes the
    /// analyzed modules and validated request identities; the caller retains
    /// the source table and parse diagnostics for report assembly.
    pub(crate) fn build(
        analyzed: BTreeMap<ProjectRelativePath, LocalArtifact>,
        module_ids: &BTreeMap<ProjectRelativePath, ModuleId>,
        resolutions: ResolutionTable,
        request_ids: &BTreeMap<ResolutionRequestKey, QualifiedRequestId>,
    ) -> Result<Self, ProjectPhaseError> {
        let mut modules = BTreeMap::new();
        for (path, local) in analyzed {
            let Some(id) = module_ids.get(&path).copied() else {
                return Err(ProjectPhaseError::InvalidTarget(path.to_string()));
            };
            modules.insert(id, ProjectModule::new(id, local));
        }

        let request_count = modules
            .values()
            .map(|module| module.local().interface().requests().count())
            .sum::<usize>();
        if request_count > MAX_PROJECT_REQUESTS {
            return Err(ProjectPhaseError::BudgetExceeded(
                "authored request count".into(),
            ));
        }
        let export_count = modules
            .values()
            .map(|module| module.local().interface().exports().count())
            .sum::<usize>();
        if export_count > MAX_EXPORT_ENTRIES {
            return Err(ProjectPhaseError::BudgetExceeded(
                "export table size".into(),
            ));
        }

        let resolutions = resolutions
            .into_iter()
            .map(|(key, result)| {
                let request = request_ids
                    .get(&key)
                    .copied()
                    .ok_or_else(|| ProjectPhaseError::UnknownRequest(key.clone()))?;
                Ok((request, resolve_record(result, module_ids)?))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        Ok(Self {
            modules,
            resolutions,
        })
    }

    pub(super) fn into_linker(self, link_limit: usize) -> ProjectLinker {
        ProjectLinker::new(self.modules, self.resolutions, link_limit)
    }

    #[cfg(test)]
    pub(crate) fn resolution_count(&self) -> usize {
        self.resolutions.len()
    }
}

fn resolve_record(
    result: ResolverOutcome,
    ids: &BTreeMap<ProjectRelativePath, ModuleId>,
) -> Result<LinkedModuleTarget, ProjectPhaseError> {
    let resolved = match result {
        ResolverOutcome::Internal { path } => {
            let Some(id) = ids.get(&path).copied() else {
                return Err(ProjectPhaseError::InvalidTarget(path.to_string()));
            };
            LinkedModuleTarget::Internal { id }
        }
        ResolverOutcome::External { package } => LinkedModuleTarget::External { package },
        ResolverOutcome::Builtin { name } => LinkedModuleTarget::Builtin { name },
        ResolverOutcome::Missing => LinkedModuleTarget::Missing,
        ResolverOutcome::OutsideProject { path } => LinkedModuleTarget::OutsideProject { path },
        ResolverOutcome::Unsupported { reason } => LinkedModuleTarget::Unsupported { reason },
    };
    Ok(resolved)
}

// ---------------------------------------------------------------------------
// ProjectSemanticModel
// ---------------------------------------------------------------------------

/// The linked, partitioned semantic model for a project. Local value and fact
/// identities remain owned by their module; the overlay stores qualified
/// resolution results rather than merging lexical arenas.
pub struct ProjectSemanticModel {
    /// Locally analyzed modules keyed by stable module ID.
    modules: BTreeMap<ModuleId, ProjectModule>,
    /// Authored request resolutions keyed by importer/span/kind.
    resolutions: BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
    /// Fixed-point export identities for linked modules.
    pub(super) exports: ExportTable,
    /// Number of unique internal edges between modules.
    pub(super) edge_count: usize,
    /// Sum of cycle-local fixed-point rounds (0 for acyclic graphs).
    pub(super) link_cycle_rounds: usize,
    /// Project diagnostics accumulated during linking and budgets.
    pub(super) diagnostics: Vec<AnalysisDiagnostic>,
    pub(super) status: AnalysisStatus,
    pub(super) flow_limit: usize,
    pub(super) effect_limit: usize,
    pub(super) trace_limit: usize,
    /// Trace storage is mutable only while projection runs, then remains
    /// immutably owned by the linked project for report assembly.
    pub(super) trace_arena: TraceArena,
}

pub(super) struct LinkedProjectState {
    pub(super) modules: BTreeMap<ModuleId, ProjectModule>,
    pub(super) resolutions: BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
    pub(super) exports: ExportTable,
    pub(super) edge_count: usize,
    pub(super) link_cycle_rounds: usize,
    pub(super) diagnostics: Vec<AnalysisDiagnostic>,
    pub(super) status: AnalysisStatus,
}

impl ProjectSemanticModel {
    pub(super) fn from_linker(state: LinkedProjectState, limits: &crate::AnalysisLimits) -> Self {
        Self {
            modules: state.modules,
            resolutions: state.resolutions,
            exports: state.exports,
            edge_count: state.edge_count,
            link_cycle_rounds: state.link_cycle_rounds,
            diagnostics: state.diagnostics,
            status: state.status,
            flow_limit: limits.flow_operations(),
            effect_limit: limits.effect_operations(),
            trace_limit: limits.trace_nodes(),
            trace_arena: TraceArena::new(limits.trace_nodes()),
        }
    }

    /// Create a project model for one already analyzed source without linking.
    #[cfg(test)]
    pub fn single(
        path: impl Into<String>,
        source: crate::analysis::LocatedSourceContext,
        local: LocalArtifact,
    ) -> Self {
        Self::single_with_limits(path, source, local, &crate::AnalysisLimits::default())
    }

    #[cfg(test)]
    fn single_with_limits(
        _path: impl Into<String>,
        _source: crate::analysis::LocatedSourceContext,
        local: LocalArtifact,
        limits: &crate::AnalysisLimits,
    ) -> Self {
        let status = local.status().clone();
        Self {
            modules: std::iter::once((
                ModuleId::new(0),
                ProjectModule::new(ModuleId::new(0), local),
            ))
            .collect(),
            resolutions: BTreeMap::new(),
            exports: ExportTable::default(),
            edge_count: 0,
            link_cycle_rounds: 0,
            diagnostics: Vec::new(),
            status,
            flow_limit: limits.flow_operations(),
            effect_limit: limits.effect_operations(),
            trace_limit: limits.trace_nodes(),
            trace_arena: TraceArena::new(limits.trace_nodes()),
        }
    }

    /// Build a linked project model from already-analyzed modules and
    /// caller-supplied resolution results. Export identities are resolved
    /// to a fixed point; flow overlays are prepared for matcher projection.
    /// Diagnoses missing or misaligned resolutions and bounded budgets.
    pub(crate) fn link_with_limits(
        link_input: ResolvedLinkInput,
        limits: &crate::AnalysisLimits,
    ) -> Self {
        let mut linker = link_input.into_linker(limits.link_operations());
        linker.propagate_local_status();
        linker.build_graph_and_exports();
        linker.finish(limits)
    }

    pub fn modules(&self) -> impl Iterator<Item = &ProjectModule> {
        self.modules.values()
    }

    pub(in crate::analysis) fn module(&self, module: ModuleId) -> Option<&ProjectModule> {
        self.modules.get(&module)
    }

    fn local_artifact(&self, module: ModuleId) -> Option<&LocalArtifact> {
        self.module(module).map(ProjectModule::local)
    }

    pub(in crate::analysis) fn resolution_for(
        &self,
        key: &QualifiedRequestId,
    ) -> Option<&LinkedModuleTarget> {
        self.resolutions.get(key)
    }

    pub(in crate::analysis) fn resolve_imported_identity(
        &self,
        importer: ModuleId,
        authored_module: &SmolStr,
        authored_export: &SmolStr,
        session: &mut LinkingSession,
    ) -> ExportResolution {
        let lookup = ProjectLookupView::new(&self.modules, &self.resolutions);
        ExportResolver::new(&lookup, &self.exports, &mut session.lookup_cache)
            .resolve_imported_identity(importer, authored_module, authored_export)
    }

    pub(in crate::analysis) fn effect(
        &self,
        target: QualifiedFunctionId,
    ) -> Option<&FunctionEffect> {
        self.local_artifact(target.module())?
            .effects()
            .get(target.function())
    }

    /// Borrow the name table for a module's local artifact.
    pub(in crate::analysis) fn module_names(&self, module: ModuleId) -> Option<&NameTable> {
        Some(self.local_artifact(module)?.facts().names())
    }

    /// Borrow the fact stream for a module's local artifact.
    pub(in crate::analysis) fn module_fact_stream(
        &self,
        module: ModuleId,
    ) -> Option<&FactStream<Frozen>> {
        Some(self.local_artifact(module)?.facts().stream())
    }

    pub(in crate::analysis) fn fact(&self, event: QualifiedEvent) -> Option<&SemanticFact> {
        self.local_artifact(event.module())?
            .facts()
            .stream()
            .fact(event.fact())
    }

    /// Return the result value produced by a source call fact, if known.
    pub(in crate::analysis) fn source_call_result(&self, event: QualifiedEvent) -> ValueId {
        self.module_fact_stream(event.module())
            .and_then(|stream| stream.fact(event.fact()))
            .map_or(ValueId::UNKNOWN, |fact| match &fact.payload {
                crate::analysis::facts::FactPayload::Call { result, .. } => *result,
                _ => ValueId::UNKNOWN,
            })
    }

    /// Convert a module/fact identity into a source location for related
    /// evidence.
    pub fn fact_location(&self, event: QualifiedEvent) -> Option<SourceLocation> {
        let module = self.module(event.module())?;
        let fact = module.local().facts().stream().fact(event.fact())?;
        let range = module.source_context().range(fact.span).ok()?;

        Some(SourceLocation::new(module.path().clone(), range))
    }

    /// Resolve a callable target across local or qualified module identities.
    pub(in crate::analysis) fn qualified_function_target(
        &self,
        importer: ModuleId,
        local: Option<FunctionId>,
        provenance: &SymbolCallProvenance,
        session: &mut LinkingSession,
    ) -> Option<QualifiedFunctionId> {
        if let Some(local) = local {
            return Some(QualifiedFunctionId::new(importer, local));
        }
        let SymbolCallProvenance::ModuleExport { module, export } = provenance else {
            return None;
        };
        let ExportResolution::Qualified {
            module: target,
            export: target_export,
        } = self.resolve_imported_identity(importer, module, export, session)
        else {
            return None;
        };
        let function = self
            .modules
            .get(&target)
            .and_then(|module| module.local().interface().function_export(&target_export));
        let function = function?;
        Some(QualifiedFunctionId::new(target, function))
    }

    /// Borrow diagnostics produced during project linking and analysis.
    pub fn diagnostics(&self) -> &[AnalysisDiagnostic] {
        &self.diagnostics
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.status.is_complete()
    }

    pub(crate) fn status_diagnostics(
        &self,
    ) -> (
        Vec<(ProjectRelativePath, AnalysisDiagnostic)>,
        Vec<AnalysisDiagnostic>,
    ) {
        self.status.diagnostics()
    }

    pub(crate) fn record_parse_failure(
        &mut self,
        path: ProjectRelativePath,
        kind: crate::parse::ParseFailureKind,
    ) {
        self.status.record(
            StatusScope::File(path),
            IncompleteReason::ParseFailure { kind },
        );
    }

    pub(in crate::analysis) fn flow_limit(&self) -> usize {
        self.flow_limit
    }

    pub(in crate::analysis) fn effect_limit(&self) -> usize {
        self.effect_limit
    }

    #[allow(dead_code)]
    pub(in crate::analysis) fn trace_limit(&self) -> usize {
        self.trace_limit
    }

    pub(crate) fn reconstruct_trace(&self, head: TraceNodeId) -> Option<Vec<TraceStep>> {
        self.trace_arena.reconstruct_trace(head)
    }

    pub(crate) fn trace_node_count(&self) -> usize {
        self.trace_arena.node_count()
    }

    /// Return deterministic phase and evidence operation counts.
    pub(crate) fn operation_counts(&self) -> crate::project::types::AnalysisOperationCountsBuilder {
        let mut counts = crate::project::types::AnalysisOperationCountsBuilder::default();
        counts.record_files(self.modules.len());
        counts.record_requests(
            self.modules
                .values()
                .map(|module| module.local().interface().requests().count())
                .sum(),
        );
        counts.record_edges(self.edge_count);
        counts.record_exports(self.exports.len());
        counts.record_scc_rounds(self.link_cycle_rounds);
        counts
    }

    pub fn classify_with_evidence_limit(
        &mut self,
        records: &[CompiledRuleRecord],
        selected: &[RuleIndex],
        evidence_limit: usize,
    ) -> (BTreeMap<ModuleId, ClassificationResult>, ProjectionOutcome) {
        let (results, outcome, arena) = {
            let (matcher_catalog, outcome, arena) =
                crate::analysis::project::projection::project_for_classification(
                    self,
                    CompiledRuleSelection::new(records, selected)
                        .expect("linter supplies a validated rule selection"),
                );
            let results = crate::analysis::project::projection::assemble_classification_results(
                &matcher_catalog,
                records,
                selected,
                evidence_limit,
            );
            (results, outcome, arena)
        };
        self.trace_arena = arena;
        (results, outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frozen semantic model must be shareable across threads so that
    /// future multi-threaded matcher projection is safe.
    #[test]
    fn semantic_model_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ProjectSemanticModel>();
    }

    /// The linking session must be sendable across threads as it owns only
    /// Send types (ExportLookupCache).
    #[test]
    fn linking_session_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<LinkingSession>();
    }
}
