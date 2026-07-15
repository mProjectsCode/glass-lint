//! Private semantic analysis and project linking.
//!
//! Local construction and matcher projection are deliberately separate. A
//! source is parsed and semantically visited once into a matcher-independent
//! model; rules query a linked project model afterwards.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

use crate::project::{
    ModuleId, ProjectInput, ProjectInputError, ResolutionRequestKey, ResolutionResult,
    ResolvedModule,
};
use project::state::{ExportTable, ModuleGraph};
use swc_common::{SourceMap, SourceMapper, sync::Lrc};

mod evidence;
mod facts;
mod flow;
mod local;
mod matching;
mod module;
mod project;
mod resolution;
mod scope;
mod syntax;
mod value;

pub(crate) use local::{LocalModuleModel, ProjectModule};

use syntax::SymbolCallProvenance;

const MAX_EXPORT_DEPTH: usize = 1024;
const MAX_GRAPH_EDGES: usize = 1_000_000;
const MAX_EXPORT_ENTRIES: usize = 1_000_000;
const MAX_SCC_SIZE: usize = 4_096;
const MAX_PROJECT_REQUESTS: usize = 500_000;

/// The linked, partitioned semantic model for a project. Local value and fact
/// identities remain owned by their module; the overlay stores qualified
/// resolution results rather than merging lexical arenas.
pub(crate) struct ProjectSemanticModel {
    modules: BTreeMap<ModuleId, ProjectModule>,
    resolutions: BTreeMap<ResolutionRequestKey, ResolvedModule>,
    exports: ExportTable,
    graph: ModuleGraph,
    link_rounds: usize,
    diagnostics: Vec<crate::ProjectDiagnostic>,
    flow_budget: crate::budget::BudgetTracker,
    link_budget: crate::budget::BudgetTracker,
    effect_projections: Cell<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ExportResolution {
    External { module: String, export: String },
    Global { name: String },
    StaticString { value: String },
    Qualified { module: ModuleId, export: String },
    Unknown,
    Ambiguous,
}

impl ProjectSemanticModel {
    pub(crate) fn single(
        path: impl Into<String>,
        source_map: Lrc<SourceMap>,
        local: LocalModuleModel,
    ) -> Self {
        Self {
            modules: [(
                ModuleId(0),
                ProjectModule::new(ModuleId(0), path.into(), source_map, local),
            )]
            .into_iter()
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
    pub(crate) fn link(
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

    pub(crate) fn modules(&self) -> impl Iterator<Item = &ProjectModule> {
        self.modules.values()
    }

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

    pub(crate) fn fact_location(
        &self,
        module: ModuleId,
        fact: u32,
    ) -> Option<crate::ProjectEvidence> {
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

    pub(crate) fn diagnostics(&self) -> &[crate::ProjectDiagnostic] {
        &self.diagnostics
    }

    pub(crate) fn flow_budget_exhausted(&self) -> bool {
        self.flow_budget.is_exhausted()
    }

    pub(crate) fn operation_counts(&self, evidence: usize) -> crate::ProjectOperationCounts {
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

    pub(crate) fn authored_requests(module: &ProjectModule) -> Vec<crate::ResolutionRequest> {
        module
            .local()
            .interface()
            .authored_requests(module.path(), module.source_map())
    }

    pub(crate) fn classify(
        &self,
        catalog: &crate::api::compiler::CompiledCatalog,
        rules: &[crate::api::rule::ApiRule],
        selected: &std::collections::BTreeSet<usize>,
    ) -> BTreeMap<ModuleId, crate::api::classification::ApiClassificationResult> {
        let matcher_catalog = self.project(catalog.to_matcher_catalog(selected));
        self.modules()
            .map(|module| {
                let mut result = crate::api::classification::ApiClassificationResult::default();
                for rule_index in 0..rules.len() {
                    if !selected.contains(&rule_index) {
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
            ExportResolution::External { module, export } => {
                matching::LinkedModuleIdentity::External { module, export }
            }
            ExportResolution::Global { name } => matching::LinkedModuleIdentity::Global { name },
            ExportResolution::StaticString { value } => {
                matching::LinkedModuleIdentity::StaticString { value }
            }
            ExportResolution::Qualified { module, export } => {
                matching::LinkedModuleIdentity::Qualified {
                    module: module.0,
                    export,
                }
            }
            ExportResolution::Unknown | ExportResolution::Ambiguous => {
                matching::LinkedModuleIdentity::Unknown
            }
        }
    }
}

fn is_internal_request(request: &str) -> bool {
    request.starts_with('.') || request.starts_with('/') || request.starts_with('#')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Environment;
    use crate::api::compiler::{CompiledMatcherCatalog, CompiledMatcherPlan};
    use crate::api::rule::ApiMatcher;
    use std::collections::BTreeSet;

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
        let selected = BTreeSet::from([0]);
        let _ = project.project(CompiledMatcherCatalog::new(vec![&fetch_plan], &selected));

        let member = ApiMatcher::from_matchers(vec![crate::api::rule::Matcher::member_call(
            crate::api::rule::MemberCallMatcher::syntactic_heuristic("document.createElement"),
        )])
        .normalized();
        let member_plan = CompiledMatcherPlan::compile(&member);
        let _ = project.project(CompiledMatcherCatalog::new(vec![&member_plan], &selected));

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
