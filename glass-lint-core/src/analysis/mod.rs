//! Private semantic analysis and project linking.
//!
//! Local construction and matcher projection are deliberately separate. A
//! source is parsed and semantically visited once into a matcher-independent
//! model; rules query a linked project model afterwards.
//!
//! Local scopes and value arenas remain partitioned by module. Linking adds
//! qualified identities and bounded flow overlays, never lexical facts from
//! one module into another.

use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
};

use project::state::{ExportTable, ModuleGraph};

use crate::project::{
    LinkedModuleTarget, ModuleId, ProjectInput, ProjectInputError, ResolutionRequestKey,
    ResolverOutcome,
};

mod evidence;
mod facts;
pub mod flow;
mod local;
mod lowering;
mod matching;
pub mod module;
pub mod project;
mod resolution;
mod scope;
mod status;
mod syntax;
mod value;

pub use value::SymbolPath;

pub fn canonical_symbol_path(value: &str) -> String {
    self::value::SymbolPath::from_chain(value).to_string()
}

pub use local::{
    ArtifactCacheHandle, ArtifactCacheKey, LocalArtifact, LocatedSourceContext, ProjectModule,
    SemanticArtifact, SharedSemanticArtifact,
};
pub use lowering::{LoweredSource, lower_artifact, lower_source};
use status::AnalysisStatus;
use syntax::SymbolCallProvenance;

const MAX_EXPORT_DEPTH: usize = 1024;
const MAX_EXPORT_ENTRIES: usize = 1_000_000;
const MAX_SCC_SIZE: usize = 4_096;
const MAX_PROJECT_REQUESTS: usize = 500_000;

/// The linked, partitioned semantic model for a project. Local value and fact
/// identities remain owned by their module; the overlay stores qualified
/// resolution results rather than merging lexical arenas.
pub struct ProjectSemanticModel {
    /// Locally analyzed modules keyed by stable module ID.
    modules: BTreeMap<ModuleId, ProjectModule>,
    /// Authored request resolutions keyed by importer/span/kind.
    resolutions: BTreeMap<ResolutionRequestKey, LinkedModuleTarget>,
    /// Fixed-point export identities for linked modules.
    exports: ExportTable,
    /// Internal module graph and strongly connected components.
    graph: ModuleGraph,
    /// Number of export-linking refinement rounds.
    link_rounds: usize,
    /// Project diagnostics accumulated during linking and budgets.
    diagnostics: Vec<crate::AnalysisDiagnostic>,
    status: std::cell::RefCell<AnalysisStatus>,
    /// Budget used by cross-module flow projection.
    flow_budget: crate::budget::BudgetTracker,
    /// Budget used by export identity linking.
    link_budget: crate::budget::BudgetTracker,
    /// Count of effect projections performed for operation telemetry.
    effect_projections: Cell<usize>,
    link_limit: usize,
    flow_limit: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ExportResolution {
    /// Identity resolved to an external module export.
    External { module: String, export: String },
    /// Identity resolved to a configured global.
    Global { name: String },
    /// Identity resolved to a static string.
    StaticString { value: String },
    /// Identity qualified to another project module.
    Qualified { module: ModuleId, export: String },
    /// Identity could not be established.
    Unknown,
    /// Multiple linked paths proved incompatible identities.
    Ambiguous,
}

struct ValidatedLinkInput {
    modules: BTreeMap<ModuleId, ProjectModule>,
    resolutions: BTreeMap<ResolutionRequestKey, LinkedModuleTarget>,
}

impl ValidatedLinkInput {
    fn build(
        input: ProjectInput,
        mut analyzed: BTreeMap<crate::ProjectRelativePath, LocalArtifact>,
    ) -> Result<Self, ProjectInputError> {
        let ids = input.module_ids();
        let mut modules = BTreeMap::new();
        for source in &input.sources {
            let Some(local) = analyzed.remove(&source.path) else {
                continue;
            };
            let Some(id) = ids.get(&source.path).copied() else {
                return Err(ProjectInputError::InvalidTarget(source.path.to_string()));
            };
            modules.insert(id, ProjectModule::new(id, local));
        }

        let authored = modules
            .values()
            .flat_map(ProjectModule::authored_requests)
            .map(|request| request.key)
            .collect::<BTreeSet<_>>();
        for (key, _) in &input.resolutions {
            if !authored.contains(key) {
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

        let resolutions = input
            .resolutions
            .into_iter()
            .map(|(key, result)| resolve_record(key, result, &ids))
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        Ok(Self {
            modules,
            resolutions,
        })
    }
}

fn resolve_record(
    key: ResolutionRequestKey,
    result: ResolverOutcome,
    ids: &BTreeMap<crate::ProjectRelativePath, ModuleId>,
) -> Result<(ResolutionRequestKey, LinkedModuleTarget), ProjectInputError> {
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
    Ok((key, resolved))
}

impl ProjectSemanticModel {
    /// Create a project model for one already analyzed source without linking.
    #[cfg(test)]
    pub fn single(
        path: impl Into<String>,
        source: LocatedSourceContext,
        local: LocalArtifact,
    ) -> Self {
        Self::single_with_limits(path, source, local, &crate::AnalysisLimits::default())
    }

    #[cfg(test)]
    fn single_with_limits(
        _path: impl Into<String>,
        _source: LocatedSourceContext,
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
            status: std::cell::RefCell::new(status),
            flow_budget: crate::budget::BudgetTracker::default(),
            link_budget: crate::budget::BudgetTracker::default(),
            effect_projections: Cell::new(0),
            link_limit: limits.link_operations,
            flow_limit: limits.flow_operations,
        }
    }

    pub fn link_with_limits(
        input: ProjectInput,
        analyzed: BTreeMap<crate::ProjectRelativePath, LocalArtifact>,
        limits: &crate::AnalysisLimits,
    ) -> Result<Self, ProjectInputError> {
        let input = input.validate()?;
        let validated = ValidatedLinkInput::build(input, analyzed)?;

        let mut project = Self {
            modules: validated.modules,
            resolutions: validated.resolutions,
            exports: ExportTable::default(),
            graph: ModuleGraph::default(),
            link_rounds: 0,
            diagnostics: Vec::new(),
            status: std::cell::RefCell::new(AnalysisStatus::default()),
            flow_budget: crate::budget::BudgetTracker::default(),
            link_budget: crate::budget::BudgetTracker::default(),
            effect_projections: Cell::new(0),
            link_limit: limits.link_operations,
            flow_limit: limits.flow_operations,
        };
        project.propagate_local_status();
        project.build_graph_and_exports();
        Ok(project)
    }

    fn propagate_local_status(&self) {
        let local_statuses = self
            .modules
            .values()
            .map(|module| module.local().status().clone())
            .collect::<Vec<_>>();
        for (module, status) in self.modules.values().zip(local_statuses) {
            self.status
                .borrow_mut()
                .extend(&status.for_file(module.path()));
            if module.local().interface().is_unknown() {
                self.status.borrow_mut().record(
                    status::StatusScope::File(module.path().clone()),
                    status::IncompleteReason::UnsupportedModuleInterface {
                        kind: status::ModuleInterfaceKind::CommonJsExports,
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
        function: crate::analysis::value::FunctionId,
    ) -> Option<&crate::analysis::flow::effect::FunctionEffect> {
        self.modules.get(&module)?.local().effects().get(function)
    }

    pub(in crate::analysis) fn fact(
        &self,
        module: ModuleId,
        fact: crate::analysis::facts::FactId,
    ) -> Option<&crate::analysis::facts::SemanticFact> {
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
        fact: crate::analysis::facts::FactId,
    ) -> crate::analysis::value::ValueId {
        self.modules
            .get(&module)
            .into_iter()
            .flat_map(|module| module.local().effects().iter_effects())
            .flat_map(crate::analysis::flow::effect::FunctionEffect::calls)
            .find(|call| call.event() == fact)
            .map_or(crate::analysis::value::ValueId::UNKNOWN, |call| {
                call.result()
            })
    }

    /// Convert a module/fact identity into reportable related evidence.
    pub fn fact_location(&self, module: ModuleId, fact: u32) -> Option<crate::Evidence> {
        let module = self.modules.get(&module)?;
        let fact = module
            .local()
            .facts()
            .stream()
            .fact(crate::analysis::facts::FactId(fact))?;
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
        local: Option<crate::analysis::value::FunctionId>,
        provenance: &SymbolCallProvenance,
    ) -> Option<(ModuleId, crate::analysis::value::FunctionId)> {
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
    pub fn diagnostics(&self) -> &[crate::AnalysisDiagnostic] {
        &self.diagnostics
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.status.borrow().is_complete()
    }

    pub(crate) fn status_diagnostics(
        &self,
    ) -> (
        Vec<(
            crate::project::ProjectRelativePath,
            crate::AnalysisDiagnostic,
        )>,
        Vec<crate::AnalysisDiagnostic>,
    ) {
        self.status.borrow().diagnostics()
    }

    pub(crate) fn record_parse_failure(
        &self,
        path: crate::project::ProjectRelativePath,
        code: &str,
    ) {
        let kind = match code {
            "source_too_large" => status::ParseFailureKind::SourceTooLarge,
            "syntax_depth_exceeded" => status::ParseFailureKind::SyntaxDepth,
            _ => status::ParseFailureKind::Syntax,
        };
        self.status.borrow_mut().record(
            status::StatusScope::File(path),
            status::IncompleteReason::ParseFailure { kind },
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
            effect_projections: self.effect_projections.get(),
            evidence,
        }
    }

    pub fn classify_with_evidence_limit(
        &self,
        catalog: &crate::api::compiler::CompiledCatalog,
        rules: &[crate::api::rule::Rule],
        selected: &[crate::api::classification::RuleIndex],
        evidence_limit: usize,
    ) -> BTreeMap<ModuleId, crate::api::classification::ClassificationResult> {
        let matcher_catalog = self.project(catalog.to_matcher_catalog(selected));
        self.modules()
            .map(|module| {
                let mut result = crate::api::classification::ClassificationResult::default();
                for rule_index in selected {
                    let index = rule_index.get();
                    if rules.get(index).is_none() {
                        continue;
                    }
                    let Some(rule) = rules.get(index) else {
                        continue;
                    };
                    let evidence =
                        matcher_catalog.evidence_for(module, *rule_index, evidence_limit);
                    if evidence.is_empty() {
                        continue;
                    }
                    result
                        .capabilities
                        .push(crate::api::classification::MatchedCapability {
                            rule_index: *rule_index,
                            id: rule.id().to_string(),
                            label: rule.description().to_string(),
                            category: rule.category().clone(),
                            severity: rule.severity(),
                            confidence: rule.confidence(),
                            evidence,
                        });
                }
                (module.id(), result)
            })
            .collect()
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
            ExportResolution::Qualified { module, export } => Self::Qualified {
                module: module.get(),
                export,
            },
            ExportResolution::Unknown | ExportResolution::Ambiguous => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Environment,
        api::{
            compiler::{CompiledMatcherPlan, CompiledRuleSelection},
            rule::MatcherSet,
        },
    };

    #[test]
    fn local_model_is_unchanged_by_matcher_projection() {
        let text = "fetch('/remote'); document.createElement('div');";
        let parsed = crate::parse(text, "projection-invariant.js").expect("source should parse");
        let coordinates = lowering::SpanNormalizer::new(parsed.source_start, text);
        let local = lowering::lower_program(
            &parsed.program,
            &Environment::default(),
            &crate::AnalysisLimits::default(),
            &coordinates,
        );
        let source = crate::SourceFile::new(
            "projection-invariant.js",
            "fetch('/remote'); document.createElement('div');",
        )
        .unwrap();
        let project = ProjectSemanticModel::single(
            "projection-invariant.js",
            local::LocatedSourceContext::new(&source),
            LocalArtifact::new(
                local::LocatedSourceContext::new(&source),
                std::sync::Arc::new(local),
            ),
        );
        let before = format!(
            "{:?}",
            project
                .modules()
                .next()
                .expect("one module")
                .local()
                .facts()
        );

        let fetch =
            MatcherSet::from_matchers(vec![crate::api::rule::Matcher::global_call("fetch")])
                .normalized();
        let fetch_plan = CompiledMatcherPlan::compile(&fetch);
        let selected = [crate::api::classification::RuleIndex::new(0)];
        let fetch_rule = crate::api::compiler::CompiledRule {
            matcher: fetch_plan,
        };
        let fetch_rules = [fetch_rule];
        let _ = project.project(CompiledRuleSelection::new(&fetch_rules, &selected));

        let member = MatcherSet::from_matchers(vec![crate::api::rule::Matcher::from(
            crate::api::rule::MemberCallMatcher::heuristic("document.createElement"),
        )])
        .normalized();
        let member_plan = CompiledMatcherPlan::compile(&member);
        let member_rule = crate::api::compiler::CompiledRule {
            matcher: member_plan,
        };
        let member_rules = [member_rule];
        let _ = project.project(CompiledRuleSelection::new(&member_rules, &selected));

        let after = format!(
            "{:?}",
            project
                .modules()
                .next()
                .expect("one module")
                .local()
                .facts()
        );
        assert_eq!(before, after);
    }
}
