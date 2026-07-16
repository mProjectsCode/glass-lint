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
use swc_common::{SourceMap, SourceMapper, sync::Lrc};

use crate::project::{
    ModuleId, ProjectInput, ProjectInputError, ResolutionRequestKey, ResolutionResult,
    ResolvedModule,
};

mod evidence;
mod facts;
pub mod flow;
pub mod local;
mod matching;
pub mod module;
pub mod project;
mod resolution;
mod scope;
mod syntax;
mod value;

pub use local::{LocalModuleModel, ProjectModule};
use syntax::SymbolCallProvenance;

const MAX_EXPORT_DEPTH: usize = 1024;
const MAX_GRAPH_EDGES: usize = 1_000_000;
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
    resolutions: BTreeMap<ResolutionRequestKey, ResolvedModule>,
    /// Fixed-point export identities for linked modules.
    exports: ExportTable,
    /// Internal module graph and strongly connected components.
    graph: ModuleGraph,
    /// Number of export-linking refinement rounds.
    link_rounds: usize,
    /// Project diagnostics accumulated during linking and budgets.
    diagnostics: Vec<crate::ProjectDiagnostic>,
    /// Budget used by cross-module flow projection.
    flow_budget: crate::budget::BudgetTracker,
    /// Budget used by export identity linking.
    link_budget: crate::budget::BudgetTracker,
    /// Count of effect projections performed for operation telemetry.
    effect_projections: Cell<usize>,
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

impl ProjectSemanticModel {
    /// Create a project model for one already analyzed source without linking.
    pub fn single(
        path: impl Into<String>,
        source_map: Lrc<SourceMap>,
        local: LocalModuleModel,
    ) -> Self {
        Self {
            modules: std::iter::once((
                ModuleId(0),
                ProjectModule::new(ModuleId(0), path.into(), source_map, local),
            ))
            .collect(),
            resolutions: BTreeMap::new(),
            exports: ExportTable::default(),
            graph: {
                let mut graph = ModuleGraph::default();
                graph.ensure_node(ModuleId(0));
                graph.set_components(vec![vec![ModuleId(0)]]);
                graph
            },
            link_rounds: 0,
            diagnostics: Vec::new(),
            flow_budget: crate::budget::BudgetTracker::default(),
            link_budget: crate::budget::BudgetTracker::default(),
            effect_projections: Cell::new(0),
        }
    }

    /// Link already-built local modules to normalized resolution records.
    /// No AST or matcher work is performed here.
    pub fn link(
        input: ProjectInput,
        mut analyzed: BTreeMap<String, (Lrc<SourceMap>, LocalModuleModel)>,
    ) -> Result<Self, ProjectInputError> {
        let input = input.validate()?;
        let ids = input.module_ids();
        let mut modules = BTreeMap::new();
        for source in &input.sources {
            let Some((source_map, local)) = analyzed.remove(&source.path) else {
                continue;
            };
            let Some(id) = ids.get(&source.path).copied() else {
                return Err(ProjectInputError::InvalidTarget(source.path.clone()));
            };
            modules.insert(
                id,
                ProjectModule::new(id, source.path.clone(), source_map, local),
            );
        }

        // Resolution records are part of the authored project contract, not a
        // free-form edge list. Validate their exact source spans after local
        // construction so repeated requests with the same specifier cannot be
        // accidentally collapsed onto the first request.
        let authored = modules
            .values()
            .flat_map(Self::authored_requests)
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
            .map(|(key, result)| {
                let resolved = match result {
                    ResolutionResult::Internal { path } => {
                        let Some(id) = ids.get(&path).copied() else {
                            return Err(ProjectInputError::InvalidTarget(path));
                        };
                        ResolvedModule::Internal { id, path }
                    }
                    ResolutionResult::External { package } => ResolvedModule::External { package },
                    ResolutionResult::Builtin { name } => ResolvedModule::Builtin { name },
                    ResolutionResult::Missing => ResolvedModule::Missing,
                    ResolutionResult::OutsideProject { path } => {
                        ResolvedModule::OutsideProject { path }
                    }
                    ResolutionResult::Unsupported { reason } => {
                        ResolvedModule::Unsupported { reason }
                    }
                };
                Ok((key, resolved))
            })
            .collect::<Result<BTreeMap<_, _>, ProjectInputError>>()?;

        let mut project = Self {
            modules,
            resolutions,
            exports: ExportTable::default(),
            graph: ModuleGraph::default(),
            link_rounds: 0,
            diagnostics: Vec::new(),
            flow_budget: crate::budget::BudgetTracker::default(),
            link_budget: crate::budget::BudgetTracker::default(),
            effect_projections: Cell::new(0),
        };
        project.build_graph_and_exports();
        Ok(project)
    }

    pub fn modules(&self) -> impl Iterator<Item = &ProjectModule> {
        self.modules.values()
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
    pub fn fact_location(&self, module: ModuleId, fact: u32) -> Option<crate::ProjectEvidence> {
        let module = self.modules.get(&module)?;
        let fact = module
            .local()
            .facts()
            .stream()
            .fact(crate::analysis::facts::FactId(fact))?;
        Some(crate::ProjectEvidence {
            message: "related semantic path event".into(),
            location: Some(crate::SourceLocation {
                path: module.path().to_owned(),
                range: crate::lint::source_range_from_span(module.source_map(), fact.span),
            }),
            source: module.source_map().span_to_snippet(fact.span).ok(),
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
            .and_then(|module| {
                module
                    .local()
                    .interface()
                    .function_exports()
                    .get(&target_export)
            })
            .copied();
        let function = function?;
        Some((target, function))
    }

    /// Borrow diagnostics produced during project linking and analysis.
    pub fn diagnostics(&self) -> &[crate::ProjectDiagnostic] {
        &self.diagnostics
    }

    /// Whether bounded cross-module flow exhausted its budget.
    pub fn flow_budget_exhausted(&self) -> bool {
        self.flow_budget.is_exhausted()
    }

    /// Return deterministic phase and evidence operation counts.
    pub fn operation_counts(&self, evidence: usize) -> crate::ProjectOperationCounts {
        crate::ProjectOperationCounts {
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

    /// Return all authored module requests with source-qualified keys.
    pub fn authored_requests(module: &ProjectModule) -> Vec<crate::ResolutionRequest> {
        module
            .local()
            .interface()
            .authored_requests(module.path(), module.source_map())
    }

    /// Classify selected rules against the linked project overlay.
    pub fn classify(
        &self,
        catalog: &crate::api::compiler::CompiledCatalog,
        rules: &[crate::api::rule::Rule],
        selected: &[usize],
    ) -> BTreeMap<ModuleId, crate::api::classification::ApiClassificationResult> {
        let matcher_catalog = self.project(catalog.to_matcher_catalog(selected));
        self.modules()
            .map(|module| {
                let mut result = crate::api::classification::ApiClassificationResult::default();
                for rule_index in 0..rules.len() {
                    if selected.binary_search(&rule_index).is_err() {
                        continue;
                    }
                    let Some(rule) = rules.get(rule_index) else {
                        continue;
                    };
                    let evidence = matcher_catalog.evidence_for(module, rule_index);
                    if evidence.is_empty() {
                        continue;
                    }
                    result
                        .capabilities
                        .push(crate::api::classification::ApiCapability {
                            rule_index,
                            id: rule.id().to_string(),
                            label: rule.label().to_string(),
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
                module: module.0,
                export,
            },
            ExportResolution::Unknown | ExportResolution::Ambiguous => Self::Unknown,
        }
    }
}

fn is_internal_request(request: &str) -> bool {
    request.starts_with('.') || request.starts_with('/') || request.starts_with('#')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Environment,
        api::{
            compiler::{CompiledMatcherCatalog, CompiledMatcherPlan},
            rule::ApiMatcher,
        },
    };

    #[test]
    fn local_model_is_unchanged_by_matcher_projection() {
        let parsed = crate::parse(
            "fetch('/remote'); document.createElement('div');",
            "projection-invariant.js",
        )
        .expect("source should parse");
        let local = LocalModuleModel::analyze(&parsed.program, &Environment::default());
        let project =
            ProjectSemanticModel::single("projection-invariant.js", parsed.source_map, local);
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
            ApiMatcher::from_matchers(vec![crate::api::rule::Matcher::global_call("fetch")])
                .normalized();
        let fetch_plan = CompiledMatcherPlan::compile(&fetch);
        let selected = [0];
        let fetch_rule = crate::api::compiler::CompiledRule {
            matcher: fetch_plan,
        };
        let fetch_rules = [fetch_rule];
        let _ = project.project(CompiledMatcherCatalog::new(&fetch_rules, &selected));

        let member = ApiMatcher::from_matchers(vec![crate::api::rule::Matcher::member_call(
            crate::api::rule::MemberCallMatcher::syntactic_heuristic("document.createElement"),
        )])
        .normalized();
        let member_plan = CompiledMatcherPlan::compile(&member);
        let member_rule = crate::api::compiler::CompiledRule {
            matcher: member_plan,
        };
        let member_rules = [member_rule];
        let _ = project.project(CompiledMatcherCatalog::new(&member_rules, &selected));

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
