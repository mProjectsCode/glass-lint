//! The linked, partitioned semantic model for a project. Local value and fact
//! identities remain owned by their module; the overlay stores qualified
//! resolution results rather than merging lexical arenas.

use std::collections::BTreeMap;

use glass_lint_datastructures::NameTable;
use smol_str::SmolStr;

use crate::{
    analysis::{
        facts::{FactId, FactStream, Frozen, SemanticFact},
        flow::effect::FunctionEffect,
        local::{LocalArtifact, ProjectModule},
        lowering::status::{AnalysisStatus, IncompleteReason, StatusScope},
        module::ModuleRequestId,
        project::{
            linker::ProjectLinker,
            projection::ProjectionOutcome,
            state::{ExportTable, LinkingSession},
        },
        syntax::SymbolCallProvenance,
        trace::TraceArena,
        value::{FunctionId, ValueId},
    },
    api::{
        classification::{ClassificationResult, MatchedCapability, RuleIndex},
        compiler::{CompiledRuleRecord, CompiledRuleSelection},
    },
    project::{
        AnalysisDiagnostic, LinkedModuleTarget, ModuleId, ProjectInputError, ProjectRelativePath,
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
    pub(crate) modules: BTreeMap<ModuleId, ProjectModule>,
    pub(crate) resolutions: BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
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
    ) -> Result<Self, ProjectInputError> {
        let mut modules = BTreeMap::new();
        for (path, local) in analyzed {
            let Some(id) = module_ids.get(&path).copied() else {
                return Err(ProjectInputError::InvalidTarget(path.to_string()));
            };
            modules.insert(id, ProjectModule::new(id, local));
        }

        let request_count = modules
            .values()
            .map(|module| module.local().interface().requests().count())
            .sum::<usize>();
        if request_count > MAX_PROJECT_REQUESTS {
            return Err(ProjectInputError::BudgetExceeded(
                "authored request count".into(),
            ));
        }
        let export_count = modules
            .values()
            .map(|module| module.local().interface().exports().count())
            .sum::<usize>();
        if export_count > MAX_EXPORT_ENTRIES {
            return Err(ProjectInputError::BudgetExceeded(
                "export table size".into(),
            ));
        }

        let resolutions = resolutions
            .into_iter()
            .map(|(key, result)| {
                let request = request_ids
                    .get(&key)
                    .copied()
                    .ok_or_else(|| ProjectInputError::UnknownRequest(key.clone()))?;
                Ok((request, resolve_record(result, module_ids)?))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        Ok(Self {
            modules,
            resolutions,
        })
    }
}

fn resolve_record(
    result: ResolverOutcome,
    ids: &BTreeMap<ProjectRelativePath, ModuleId>,
) -> Result<LinkedModuleTarget, ProjectInputError> {
    let resolved = match result {
        ResolverOutcome::Internal { path } => {
            let Some(id) = ids.get(&path).copied() else {
                return Err(ProjectInputError::InvalidTarget(path.to_string()));
            };
            LinkedModuleTarget::Internal { id, path }
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
    pub(super) modules: BTreeMap<ModuleId, ProjectModule>,
    /// Authored request resolutions keyed by importer/span/kind.
    pub(super) resolutions: BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
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

impl ProjectSemanticModel {
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
        let mut linker = ProjectLinker::new(
            link_input.modules,
            link_input.resolutions,
            limits.link_operations(),
        );
        linker.propagate_local_status();
        linker.build_graph_and_exports();
        linker.finish(limits)
    }

    pub fn modules(&self) -> impl Iterator<Item = &ProjectModule> {
        self.modules.values()
    }

    pub(in crate::analysis) fn effect(
        &self,
        module: ModuleId,
        function: FunctionId,
    ) -> Option<&FunctionEffect> {
        self.modules.get(&module)?.local().effects().get(function)
    }

    /// Borrow the name table for a module's local artifact.
    pub(in crate::analysis) fn module_names(&self, module: ModuleId) -> Option<&NameTable> {
        Some(self.modules.get(&module)?.local().facts().names())
    }

    /// Borrow the fact stream for a module's local artifact.
    pub(in crate::analysis) fn module_fact_stream(
        &self,
        module: ModuleId,
    ) -> Option<&FactStream<Frozen>> {
        Some(self.modules.get(&module)?.local().facts().stream())
    }

    pub(in crate::analysis) fn fact(
        &self,
        module: ModuleId,
        fact: FactId,
    ) -> Option<&SemanticFact> {
        self.modules
            .get(&module)?
            .local()
            .facts()
            .stream()
            .fact(fact)
    }

    /// Return the result value produced by a source call fact, if known.
    pub(in crate::analysis) fn source_call_result(
        &self,
        module: ModuleId,
        fact: FactId,
    ) -> ValueId {
        self.module_fact_stream(module)
            .and_then(|stream| stream.fact(fact))
            .map_or(ValueId::UNKNOWN, |fact| match &fact.payload {
                crate::analysis::facts::FactPayload::Call { result, .. } => *result,
                _ => ValueId::UNKNOWN,
            })
    }

    /// Convert a module/fact identity into a source location for related
    /// evidence.
    pub fn fact_location(&self, module: ModuleId, fact: FactId) -> Option<SourceLocation> {
        let module = self.modules.get(&module)?;
        let fact = module.local().facts().stream().fact(fact)?;
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
    ) -> Option<(ModuleId, FunctionId)> {
        if let Some(local) = local {
            return Some((importer, local));
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
        Some((target, function))
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

    pub fn trace_arena(&self) -> &TraceArena {
        &self.trace_arena
    }

    /// Return deterministic phase and evidence operation counts.
    pub fn operation_counts(&self, evidence: usize) -> crate::project::AnalysisOperationCounts {
        crate::project::AnalysisOperationCounts::new(
            self.modules.len(),
            self.modules
                .values()
                .map(|module| module.local().interface().requests().count())
                .sum(),
            self.edge_count,
            self.exports.len(),
            self.link_cycle_rounds,
            0,
            evidence,
        )
    }

    pub fn classify_with_evidence_limit(
        &mut self,
        records: &[CompiledRuleRecord],
        selected: &[RuleIndex],
        evidence_limit: usize,
    ) -> (BTreeMap<ModuleId, ClassificationResult>, ProjectionOutcome) {
        let trace_limit = self.trace_limit;
        let mut arena = std::mem::replace(&mut self.trace_arena, TraceArena::new(trace_limit));
        let (results, outcome) = {
            let (matcher_catalog, outcome) =
                self.project_with_arena(CompiledRuleSelection::new(records, selected), &mut arena);
            let results = self
                .modules()
                .map(|module| {
                    let mut result = ClassificationResult::default();
                    for rule_index in selected {
                        let index = rule_index.get();
                        let Some(record) = records.get(index) else {
                            continue;
                        };
                        let evidence =
                            matcher_catalog.evidence_for(module, *rule_index, evidence_limit);
                        if evidence.is_empty() {
                            continue;
                        }

                        result.capabilities.push(MatchedCapability {
                            rule_index: *rule_index,
                            label: record.description.clone(),
                            severity: record.severity,
                            evidence,
                        });
                    }
                    (module.id(), result)
                })
                .collect();
            (results, outcome)
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
