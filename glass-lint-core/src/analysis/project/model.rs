//! The linked, partitioned semantic model for a project. Local value and fact
//! identities remain owned by their module; the overlay stores qualified
//! resolution results rather than merging lexical arenas.

use std::collections::BTreeMap;

use smol_str::SmolStr;

use super::{
    projection::ProjectionOutcome,
    state::{ExportTable, ModuleGraph},
};
use crate::{
    AnalysisDiagnostic, ProjectRelativePath,
    analysis::{
        facts::{FactId, FactStream, SemanticFact},
        flow::effect::FunctionEffect,
        local::{LocalArtifact, ProjectModule},
        matching,
        module::ModuleRequestId,
        name::NameTable,
        status::{
            AnalysisStatus, IncompleteReason, ModuleInterfaceKind, ParseFailureKind, StatusScope,
        },
        syntax::SymbolCallProvenance,
        value::{FunctionId, ValueId},
    },
    api::{
        classification::{ClassificationResult, MatchedCapability, RuleIndex},
        compiler::{CompiledCatalog, CompiledRuleRecord},
    },
    budget::BudgetTracker,
    project::{
        LinkedModuleTarget, ModuleId, ProjectInputError, ResolverOutcome,
        input::ValidatedProjectInput,
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
pub(in crate::analysis) struct QualifiedRequestId {
    pub(in crate::analysis) module: ModuleId,
    pub(in crate::analysis) request: ModuleRequestId,
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

// ---------------------------------------------------------------------------
// ValidatedLinkInput (private helper)
// ---------------------------------------------------------------------------

struct ValidatedLinkInput {
    modules: BTreeMap<ModuleId, ProjectModule>,
    resolutions: BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
}

impl ValidatedLinkInput {
    fn build(
        input: ValidatedProjectInput,
        mut analyzed: BTreeMap<ProjectRelativePath, LocalArtifact>,
    ) -> Result<Self, ProjectInputError> {
        let (root, source_map, resolution_map, module_ids) = input.into_parts();
        let mut modules = BTreeMap::new();
        for path in source_map.keys() {
            let Some(local) = analyzed.remove(path) else {
                continue;
            };
            let Some(id) = module_ids.get(path).copied() else {
                return Err(ProjectInputError::InvalidTarget(path.to_string()));
            };
            modules.insert(id, ProjectModule::new(id, local));
        }

        drop(root);

        let authored = modules
            .values()
            .flat_map(|module| {
                module
                    .authored_requests_with_ids()
                    .into_iter()
                    .map(move |(request, authored)| {
                        (
                            authored.key,
                            QualifiedRequestId {
                                module: module.id(),
                                request,
                            },
                        )
                    })
            })
            .collect::<BTreeMap<_, _>>();
        for key in resolution_map.keys() {
            if !authored.contains_key(key) {
                return Err(ProjectInputError::UnknownRequest(key.clone()));
            }
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

        let resolutions = resolution_map
            .into_iter()
            .map(|(key, result)| {
                let request = authored
                    .get(&key)
                    .copied()
                    .ok_or_else(|| ProjectInputError::UnknownRequest(key.clone()))?;
                Ok((request, resolve_record(result, &module_ids)?))
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
    /// Internal module graph and strongly connected components.
    pub(super) graph: ModuleGraph,
    /// Number of export-linking refinement rounds.
    pub(super) link_rounds: usize,
    /// Project diagnostics accumulated during linking and budgets.
    pub(super) diagnostics: Vec<AnalysisDiagnostic>,
    pub(super) status: AnalysisStatus,
    /// Budget used by export identity linking.
    pub(super) link_budget: BudgetTracker,
    pub(super) link_limit: usize,
    pub(super) flow_limit: usize,
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
            graph: {
                let mut graph = ModuleGraph::default();
                graph.ensure_node(ModuleId::new(0));
                graph.set_components(vec![vec![ModuleId::new(0)]]);
                graph
            },
            link_rounds: 0,
            diagnostics: Vec::new(),
            status,
            link_budget: BudgetTracker::default(),
            link_limit: limits.link_operations(),
            flow_limit: limits.flow_operations(),
        }
    }

    /// Build a linked project model from already-analyzed modules and
    /// caller-supplied resolution results. Export identities are resolved
    /// to a fixed point; flow overlays are prepared for matcher projection.
    /// Diagnoses missing or misaligned resolutions and bounded budgets.
    pub(crate) fn link_with_limits(
        input: ValidatedProjectInput,
        analyzed: BTreeMap<ProjectRelativePath, LocalArtifact>,
        limits: &crate::AnalysisLimits,
    ) -> Result<Self, ProjectInputError> {
        let validated = ValidatedLinkInput::build(input, analyzed)?;

        let mut project = Self {
            modules: validated.modules,
            resolutions: validated.resolutions,
            exports: ExportTable::default(),
            graph: ModuleGraph::default(),
            link_rounds: 0,
            diagnostics: Vec::new(),
            status: AnalysisStatus::default(),
            link_budget: BudgetTracker::default(),
            link_limit: limits.link_operations(),
            flow_limit: limits.flow_operations(),
        };
        project.propagate_local_status();
        project.build_graph_and_exports();
        Ok(project)
    }

    fn propagate_local_status(&mut self) {
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
                    StatusScope::File(path),
                    IncompleteReason::UnsupportedModuleInterface {
                        kind: ModuleInterfaceKind::CommonJsExports,
                    },
                );
            }
        }
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
        self.modules.get(&module)?.local().facts().names()
    }

    /// Borrow the fact stream for a module's local artifact.
    pub(in crate::analysis) fn module_fact_stream(&self, module: ModuleId) -> Option<&FactStream> {
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

    /// Convert a module/fact identity into reportable related evidence.
    pub fn fact_location(&self, module: ModuleId, fact: u32) -> Option<crate::Evidence> {
        let module = self.modules.get(&module)?;
        let fact = module.local().facts().stream().fact(FactId(fact))?;
        let range = module.source_context().range(fact.span).ok()?;
        Some(crate::Evidence {
            message: "related semantic path event".into(),
            count: 1,
            evidence_truncated: false,
            location: Some(crate::SourceLocation {
                path: module.path().clone(),
                range,
            }),
        })
    }

    /// Resolve a callable target across local or qualified module identities.
    pub(in crate::analysis) fn qualified_function_target(
        &self,
        importer: ModuleId,
        local: Option<FunctionId>,
        provenance: &SymbolCallProvenance,
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
        } = self.resolve_imported_identity(importer, module, export)
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

    pub(crate) fn record_parse_failure(&mut self, path: ProjectRelativePath, code: &str) {
        let kind = match code {
            "source_too_large" => ParseFailureKind::SourceTooLarge,
            "syntax_depth_exceeded" => ParseFailureKind::SyntaxDepth,
            _ => ParseFailureKind::Syntax,
        };
        self.status.record(
            StatusScope::File(path),
            IncompleteReason::ParseFailure { kind },
        );
    }

    pub(in crate::analysis) fn link_limit(&self) -> usize {
        self.link_limit
    }

    pub(in crate::analysis) fn flow_limit(&self) -> usize {
        self.flow_limit
    }

    /// Return deterministic phase and evidence operation counts.
    pub fn operation_counts(&self, evidence: usize) -> crate::AnalysisOperationCounts {
        crate::AnalysisOperationCounts {
            files: self.modules.len(),
            requests: self
                .modules
                .values()
                .map(|module| module.local().interface().requests().count())
                .sum(),
            edges: self.graph.edge_count(),
            exports: self.exports.len(),
            scc_rounds: self.link_rounds,
            effect_projections: 0,
            evidence,
        }
    }

    pub fn classify_with_evidence_limit(
        &self,
        catalog: &CompiledCatalog,
        records: &[CompiledRuleRecord],
        selected: &[RuleIndex],
        evidence_limit: usize,
    ) -> (BTreeMap<ModuleId, ClassificationResult>, ProjectionOutcome) {
        let (matcher_catalog, outcome) = self.project(catalog.to_matcher_catalog(selected));
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
                        id: record.id.clone(),
                        label: record.description.clone(),
                        category: record.category.clone(),
                        severity: record.severity,
                        confidence: record.confidence,
                        evidence,
                    });
                }
                (module.id(), result)
            })
            .collect();
        (results, outcome)
    }

    pub(in crate::analysis) fn static_export_string(
        &self,
        module: ModuleId,
        export: &str,
    ) -> Option<String> {
        self.modules
            .get(&module)
            .and_then(|module| module.local().interface().static_string(export))
            .cloned()
    }
}

impl From<ExportResolution> for matching::LinkedModuleIdentity {
    fn from(resolution: ExportResolution) -> Self {
        match resolution {
            ExportResolution::External { module, export } => Self::External { module, export },
            ExportResolution::Global { name } => Self::Global { name },
            ExportResolution::StaticString { value } => Self::StaticString { value },
            ExportResolution::Qualified { module, export } => Self::Qualified { module, export },
            ExportResolution::Ambiguous => Self::Ambiguous,
            ExportResolution::Unknown => Self::Unknown,
        }
    }
}
