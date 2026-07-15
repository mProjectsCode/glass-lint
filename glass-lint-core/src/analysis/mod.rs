//! Private semantic analysis and project linking.
//!
//! Local construction and matcher projection are deliberately separate. A
//! source is parsed and semantically visited once into a matcher-independent
//! model; rules query a linked project model afterwards.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

use swc_common::{SourceMap, SourceMapper, Spanned, sync::Lrc};
use swc_ecma_ast::Program;

use crate::Environment;
use crate::api::classification::ApiEvidence;
use crate::api::compiler::CompiledMatcherCatalog;
use crate::project::{
    ModuleId, ProjectInput, ProjectInputError, ResolutionRequestKey, ResolutionResult,
    ResolvedModule,
};

mod evidence;
mod facts;
mod flow;
mod matching;
mod module;
mod resolution;
mod scope;
mod syntax;
mod value;

use facts::SemanticFacts;
use matching::MatcherFacts;
use module::{ModuleInterface, ModuleRequestRole};
use syntax::SymbolCallProvenance;

/// The immutable, matcher-independent result of analyzing one source.
#[derive(Debug)]
pub(crate) struct LocalModuleModel {
    facts: SemanticFacts,
    export_origins: BTreeMap<String, SymbolCallProvenance>,
    pub(crate) effects: flow::effect::FunctionEffects,
}

impl LocalModuleModel {
    pub(crate) fn analyze(program: &Program, environment: &Environment) -> Self {
        let resolver = resolution::Resolver::collect_with_environment(program, environment);
        let facts = SemanticFacts::build(program, &resolver);
        let export_origins = facts
            .interface
            .exports
            .values()
            .filter_map(|declaration| match declaration {
                module::ModuleExport::Local { name } => Some((
                    name.clone(),
                    resolver.exported_provenance(name, program.span()),
                )),
                module::ModuleExport::Value
                | module::ModuleExport::ReExport { .. }
                | module::ModuleExport::Namespace { .. }
                | module::ModuleExport::Unknown => None,
            })
            .collect();
        let effects = flow::effect::collect(&facts.stream);
        Self {
            facts,
            export_origins,
            effects,
        }
    }

    pub(crate) fn interface(&self) -> &ModuleInterface {
        &self.facts.interface
    }
}

/// A successfully analyzed source together with the data needed to report
/// findings in its original file.
pub(crate) struct ProjectModule {
    pub(crate) id: ModuleId,
    pub(crate) path: String,
    pub(crate) source_map: Lrc<SourceMap>,
    pub(crate) local: LocalModuleModel,
}

/// The linked, partitioned semantic model for a project. Local value and fact
/// identities remain owned by their module; the overlay stores qualified
/// resolution results rather than merging lexical arenas.
pub(crate) struct ProjectSemanticModel {
    modules: BTreeMap<ModuleId, ProjectModule>,
    resolutions: BTreeMap<ResolutionRequestKey, ResolvedModule>,
    exports: BTreeMap<(ModuleId, String), ExportResolution>,
    adjacency: BTreeMap<ModuleId, Vec<ModuleId>>,
    reverse_adjacency: BTreeMap<ModuleId, Vec<ModuleId>>,
    components: Vec<Vec<ModuleId>>,
    link_rounds: usize,
    diagnostics: Vec<crate::ProjectDiagnostic>,
    flow_budget_exhausted: Cell<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ExportResolution {
    External { module: String, export: String },
    Global { name: String },
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
                ProjectModule {
                    id: ModuleId(0),
                    path: path.into(),
                    source_map,
                    local,
                },
            )]
            .into_iter()
            .collect(),
            resolutions: BTreeMap::new(),
            exports: BTreeMap::new(),
            adjacency: BTreeMap::new(),
            reverse_adjacency: BTreeMap::new(),
            components: vec![vec![ModuleId(0)]],
            link_rounds: 0,
            diagnostics: Vec::new(),
            flow_budget_exhausted: Cell::new(false),
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
                ProjectModule {
                    id,
                    path: source.path.clone(),
                    source_map,
                    local,
                },
            );
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
            exports: BTreeMap::new(),
            adjacency: BTreeMap::new(),
            reverse_adjacency: BTreeMap::new(),
            components: Vec::new(),
            link_rounds: 0,
            diagnostics: Vec::new(),
            flow_budget_exhausted: Cell::new(false),
        };
        project.build_graph_and_exports();
        Ok(project)
    }

    pub(crate) fn modules(&self) -> impl Iterator<Item = &ProjectModule> {
        self.modules.values()
    }

    pub(crate) fn fact_location(
        &self,
        module: ModuleId,
        fact: u32,
    ) -> Option<crate::ProjectEvidence> {
        let module = self.modules.get(&module)?;
        let fact = module
            .local
            .facts
            .stream
            .fact(crate::analysis::facts::FactId(fact))?;
        Some(crate::ProjectEvidence {
            message: "related semantic path event".into(),
            location: Some(crate::SourceLocation {
                path: module.path.clone(),
                range: crate::lint::source_range_from_span(&module.source_map, fact.span),
            }),
            source: module.source_map.span_to_snippet(fact.span).ok(),
        })
    }

    pub(crate) fn qualified_function_target(
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
                    .local
                    .interface()
                    .function_exports
                    .get(&target_export)
            })
            .copied()?;
        Some((target, function))
    }

    #[allow(dead_code)]
    pub(crate) fn resolution(&self, key: &ResolutionRequestKey) -> Option<&ResolvedModule> {
        self.resolutions.get(key)
    }

    pub(crate) fn diagnostics(&self) -> &[crate::ProjectDiagnostic] {
        &self.diagnostics
    }

    pub(crate) fn flow_budget_exhausted(&self) -> bool {
        self.flow_budget_exhausted.get()
    }

    pub(crate) fn operation_counts(&self, evidence: usize) -> crate::ProjectOperationCounts {
        crate::ProjectOperationCounts {
            files: self.modules.len(),
            requests: self
                .modules
                .values()
                .map(|module| module.local.interface().requests.len())
                .sum(),
            edges: self.adjacency.values().map(Vec::len).sum(),
            exports: self.exports.len(),
            scc_rounds: self.link_rounds,
            effect_projections: self
                .modules
                .values()
                .map(|module| module.local.effects.by_id.len())
                .sum(),
            evidence,
        }
    }

    pub(crate) fn authored_requests(module: &ProjectModule) -> Vec<crate::ResolutionRequest> {
        module
            .local
            .interface()
            .authored_requests(&module.path, &module.source_map)
    }

    #[allow(clippy::too_many_lines)]
    fn build_graph_and_exports(&mut self) {
        for module in self.modules.values() {
            self.adjacency.entry(module.id).or_default();
            if module.local.interface().unknown_exports {
                self.diagnostics.push(crate::ProjectDiagnostic {
                    code: "unsupported_commonjs_exports".into(),
                    message: format!(
                        "CommonJS export shape in `{}` is dynamic or ambiguous",
                        module.path
                    ),
                    location: None,
                });
            }
            for request in Self::authored_requests(module) {
                let Some(resolution) = self.resolutions.get(&request.key) else {
                    if is_internal_request(&request.request) {
                        self.diagnostics.push(crate::ProjectDiagnostic {
                            code: "unresolved_internal_request".into(),
                            message: format!(
                                "internal-looking module request `{}` has no resolution",
                                request.request
                            ),
                            location: Some(crate::SourceLocation {
                                path: module.path.clone(),
                                range: request.key.range.clone(),
                            }),
                        });
                    }
                    continue;
                };
                if let ResolvedModule::Internal { id, .. } = resolution {
                    self.adjacency.entry(module.id).or_default().push(*id);
                    self.reverse_adjacency
                        .entry(*id)
                        .or_default()
                        .push(module.id);
                } else if matches!(resolution, ResolvedModule::Missing)
                    && is_internal_request(&request.request)
                {
                    self.diagnostics.push(crate::ProjectDiagnostic {
                        code: "unresolved_internal_request".into(),
                        message: format!(
                            "internal module request `{}` is missing",
                            request.request
                        ),
                        location: Some(crate::SourceLocation {
                            path: module.path.clone(),
                            range: request.key.range.clone(),
                        }),
                    });
                } else if matches!(resolution, ResolvedModule::OutsideProject { .. }) {
                    self.diagnostics.push(crate::ProjectDiagnostic {
                        code: "outside_project_target".into(),
                        message: format!(
                            "module request `{}` resolves outside the project",
                            request.request
                        ),
                        location: Some(crate::SourceLocation {
                            path: module.path.clone(),
                            range: request.key.range.clone(),
                        }),
                    });
                } else if matches!(resolution, ResolvedModule::Unsupported { .. }) {
                    self.diagnostics.push(crate::ProjectDiagnostic {
                        code: "unsupported_project_target".into(),
                        message: format!(
                            "module request `{}` is not an analyzable project target",
                            request.request
                        ),
                        location: Some(crate::SourceLocation {
                            path: module.path.clone(),
                            range: request.key.range.clone(),
                        }),
                    });
                }
            }
        }
        for targets in self.adjacency.values_mut() {
            targets.sort_unstable();
            targets.dedup();
        }
        for sources in self.reverse_adjacency.values_mut() {
            sources.sort_unstable();
            sources.dedup();
        }
        self.components =
            strongly_connected_components(&self.adjacency, self.modules.keys().copied());

        // Resolve exports in a bounded, monotone pass.  The fixed point is
        // intentionally conservative: a cycle that does not stabilize stays
        // unknown instead of depending on module iteration order.
        let mut changed = true;
        let mut rounds = 0;
        while changed && rounds < self.modules.len().saturating_add(1) {
            changed = false;
            rounds += 1;
            for module in self.modules.values() {
                for (name, export) in &module.local.interface().exports {
                    let resolved = self.resolve_export(module.id, export);
                    let key = (module.id, name.clone());
                    if self.exports.get(&key) != Some(&resolved) {
                        self.exports.insert(key, resolved);
                        changed = true;
                    }
                }
            }
        }
        self.link_rounds = rounds;
        if changed {
            self.diagnostics.push(crate::ProjectDiagnostic {
                code: "graph_link_budget_exhausted".into(),
                message: "module export linking did not reach a stable fixed point".into(),
                location: None,
            });
            for value in self.exports.values_mut() {
                *value = ExportResolution::Unknown;
            }
        }

        for module in self.modules.values() {
            for request in &module.local.interface().requests {
                let key = crate::project::ResolutionRequestKey {
                    importer: module.path.clone(),
                    kind: request.kind,
                    range: crate::lint::source_range_from_span(&module.source_map, request.span),
                };
                let Some(ResolvedModule::Internal { id, .. }) = self.resolutions.get(&key) else {
                    continue;
                };
                let ModuleRequestRole::Import { bindings } = &request.role else {
                    continue;
                };
                for binding in bindings.iter().filter(|binding| !binding.namespace) {
                    let Some(imported) = binding.imported.as_deref() else {
                        continue;
                    };
                    match self.lookup_export(*id, imported, &mut std::collections::BTreeSet::new())
                    {
                        Some(ExportResolution::Ambiguous) => {
                            self.diagnostics.push(crate::ProjectDiagnostic {
                                code: "ambiguous_star_export".into(),
                                message: format!("module export `{imported}` is ambiguous"),
                                location: Some(crate::SourceLocation {
                                    path: module.path.clone(),
                                    range: key.range.clone(),
                                }),
                            });
                        }
                        None => self.diagnostics.push(crate::ProjectDiagnostic {
                            code: "missing_imported_export".into(),
                            message: format!("module does not export `{imported}`"),
                            location: Some(crate::SourceLocation {
                                path: module.path.clone(),
                                range: key.range.clone(),
                            }),
                        }),
                        Some(_) => {}
                    }
                }
            }
        }
        self.diagnostics.sort_by(|left, right| {
            (
                &left.code,
                left.location.as_ref().map(|l| &l.path),
                left.location.as_ref().map(|l| &l.range),
            )
                .cmp(&(
                    &right.code,
                    right.location.as_ref().map(|l| &l.path),
                    right.location.as_ref().map(|l| &l.range),
                ))
        });
        self.diagnostics.dedup();
    }

    fn resolve_export(&self, module: ModuleId, export: &module::ModuleExport) -> ExportResolution {
        match export {
            module::ModuleExport::Local { name } => {
                let Some(project_module) = self.modules.get(&module) else {
                    return ExportResolution::Unknown;
                };
                if !project_module.local.interface().locals.contains(name)
                    && !project_module.local.export_origins.contains_key(name)
                {
                    return ExportResolution::Unknown;
                }
                match project_module.local.export_origins.get(name) {
                    Some(SymbolCallProvenance::ModuleExport {
                        module: authored_module,
                        export: authored_export,
                    }) => self.resolve_imported_identity(module, authored_module, authored_export),
                    Some(SymbolCallProvenance::Global { name }) => {
                        ExportResolution::Global { name: name.clone() }
                    }
                    Some(SymbolCallProvenance::Local) | None => ExportResolution::Qualified {
                        module,
                        export: name.clone(),
                    },
                }
            }
            module::ModuleExport::Value => ExportResolution::Qualified {
                module,
                export: "default".into(),
            },
            module::ModuleExport::Unknown => ExportResolution::Unknown,
            module::ModuleExport::ReExport { request, imported } => {
                self.resolve_request_export(module, *request, imported)
            }
            module::ModuleExport::Namespace { request } => {
                let Some(request) = self
                    .modules
                    .get(&module)
                    .and_then(|m| m.local.interface().requests.get(*request))
                else {
                    return ExportResolution::Unknown;
                };
                let key = ResolutionRequestKey {
                    importer: self.modules[&module].path.clone(),
                    kind: request.kind,
                    range: crate::lint::source_range_from_span(
                        &self.modules[&module].source_map,
                        request.span,
                    ),
                };
                match self.resolutions.get(&key) {
                    Some(ResolvedModule::Internal { id, .. }) => ExportResolution::Qualified {
                        module: *id,
                        export: "*".into(),
                    },
                    Some(ResolvedModule::External { package }) => ExportResolution::External {
                        module: package.clone(),
                        export: "*".into(),
                    },
                    Some(ResolvedModule::Builtin { name }) => ExportResolution::External {
                        module: name.clone(),
                        export: "*".into(),
                    },
                    _ => ExportResolution::Unknown,
                }
            }
        }
    }

    fn resolve_imported_identity(
        &self,
        importer: ModuleId,
        authored_module: &str,
        authored_export: &str,
    ) -> ExportResolution {
        let requests = self
            .modules
            .get(&importer)
            .into_iter()
            .flat_map(|module| module.local.interface().requests.iter())
            .filter(|request| {
                request.specifier == authored_module
                    && matches!(
                        request.role,
                        ModuleRequestRole::Import { .. } | ModuleRequestRole::Require
                    )
            })
            .collect::<Vec<_>>();
        if requests.is_empty() {
            // A bare package with no resolver answer is intentionally opaque
            // external provenance. This preserves isolated-file behavior.
            return ExportResolution::External {
                module: authored_module.to_string(),
                export: authored_export.to_string(),
            };
        }

        // The semantic provenance format predates qualified request spans and
        // keys imports by authored module/export. If a virtual caller supplies
        // conflicting answers for repeated requests with the same specifier,
        // preserve precision by treating the identity as ambiguous instead of
        // selecting the first source-order request.
        let mut resolved = None;
        for request in requests {
            let key = self.request_key(importer, request);
            let candidate = match self.resolutions.get(&key) {
                None if is_internal_request(authored_module) => ExportResolution::Unknown,
                None => ExportResolution::External {
                    module: authored_module.to_string(),
                    export: authored_export.to_string(),
                },
                Some(ResolvedModule::External { package }) => ExportResolution::External {
                    module: package.clone(),
                    export: authored_export.to_string(),
                },
                Some(ResolvedModule::Builtin { name }) => ExportResolution::External {
                    module: name.clone(),
                    export: authored_export.to_string(),
                },
                Some(ResolvedModule::Internal { id, .. }) => self
                    .lookup_export(*id, authored_export, &mut BTreeSet::new())
                    .unwrap_or(ExportResolution::Unknown),
                Some(
                    ResolvedModule::Missing
                    | ResolvedModule::OutsideProject { .. }
                    | ResolvedModule::Unsupported { .. },
                ) => ExportResolution::Unknown,
            };
            if let Some(previous) = &resolved {
                if previous != &candidate {
                    return ExportResolution::Unknown;
                }
            } else {
                resolved = Some(candidate);
            }
        }
        resolved.unwrap_or(ExportResolution::Unknown)
    }

    fn request_key(
        &self,
        module: ModuleId,
        request: &module::ModuleRequest,
    ) -> ResolutionRequestKey {
        ResolutionRequestKey {
            importer: self.modules[&module].path.clone(),
            kind: request.kind,
            range: crate::lint::source_range_from_span(
                &self.modules[&module].source_map,
                request.span,
            ),
        }
    }

    fn lookup_export(
        &self,
        module: ModuleId,
        name: &str,
        visiting: &mut std::collections::BTreeSet<(ModuleId, String)>,
    ) -> Option<ExportResolution> {
        if !visiting.insert((module, name.to_string())) {
            return None;
        }
        if let Some(resolved) = self.exports.get(&(module, name.to_string())) {
            return Some(resolved.clone());
        }
        // ECMAScript `export *` intentionally does not forward `default`.
        if name == "default" {
            return None;
        }
        let interface = self.modules.get(&module).map(|m| m.local.interface())?;
        if interface.unknown_exports {
            return Some(ExportResolution::Unknown);
        }
        let mut candidate = None;
        let mut saw_unknown = false;
        for request_index in &interface.star_exports {
            let Some(request) = interface.requests.get(*request_index) else {
                saw_unknown = true;
                continue;
            };
            let key = ResolutionRequestKey {
                importer: self.modules[&module].path.clone(),
                kind: request.kind,
                range: crate::lint::source_range_from_span(
                    &self.modules[&module].source_map,
                    request.span,
                ),
            };
            let resolution = self.resolutions.get(&key);
            let candidate_export = match resolution {
                Some(ResolvedModule::Internal { id, .. }) => {
                    self.lookup_export(*id, name, visiting)
                }
                Some(ResolvedModule::External { package }) => Some(ExportResolution::External {
                    module: package.clone(),
                    export: name.to_string(),
                }),
                Some(ResolvedModule::Builtin { name: package }) => {
                    Some(ExportResolution::External {
                        module: package.clone(),
                        export: name.to_string(),
                    })
                }
                _ => None,
            };
            match candidate_export {
                Some(resolved)
                    if candidate
                        .as_ref()
                        .is_none_or(|existing| existing == &resolved) =>
                {
                    candidate = Some(resolved);
                }
                Some(_) => return Some(ExportResolution::Ambiguous),
                None => saw_unknown = true,
            }
        }
        if saw_unknown { None } else { candidate }
    }

    fn resolve_request_export(
        &self,
        module: ModuleId,
        request_index: usize,
        imported: &str,
    ) -> ExportResolution {
        let Some(request) = self
            .modules
            .get(&module)
            .and_then(|m| m.local.interface().requests.get(request_index))
        else {
            return ExportResolution::Unknown;
        };
        let key = ResolutionRequestKey {
            importer: self.modules[&module].path.clone(),
            kind: request.kind,
            range: crate::lint::source_range_from_span(
                &self.modules[&module].source_map,
                request.span,
            ),
        };
        match self.resolutions.get(&key) {
            Some(ResolvedModule::Internal { id, .. }) => self
                .lookup_export(*id, imported, &mut std::collections::BTreeSet::new())
                .unwrap_or(ExportResolution::Unknown),
            Some(ResolvedModule::External { package }) => ExportResolution::External {
                module: package.clone(),
                export: imported.to_string(),
            },
            Some(ResolvedModule::Builtin { name }) => ExportResolution::External {
                module: name.clone(),
                export: imported.to_string(),
            },
            _ => ExportResolution::Unknown,
        }
    }

    pub(crate) fn project<'matchers>(
        &self,
        matchers: CompiledMatcherCatalog<'matchers>,
    ) -> ProjectMatcherModel<'matchers> {
        let projections: BTreeMap<ModuleId, ProjectModuleProjection> = self
            .modules
            .values()
            .map(|module| {
                let mut facts = module.local.facts.index.clone();
                let identities = self.module_identities(module.id);
                facts.apply_module_overlay(&identities);
                (
                    module.id,
                    ProjectModuleProjection {
                        index: facts,
                        arguments: module.local.facts.project(&matchers, Some(&identities)),
                    },
                )
            })
            .collect();
        let (cross, exhausted) = flow::cross::collect(self, &matchers);
        self.flow_budget_exhausted.set(exhausted);
        let mut projections = projections;
        for (module, evidence) in cross {
            if let Some(projection) = projections.get_mut(&module) {
                for (rule, values) in evidence.into_iter().enumerate() {
                    projection.arguments[rule].extend(values);
                }
            }
        }
        ProjectMatcherModel {
            matchers,
            projections,
        }
    }

    fn module_identities(
        &self,
        module: ModuleId,
    ) -> BTreeMap<(String, String), matching::LinkedModuleIdentity> {
        let mut identities = BTreeMap::new();
        let Some(project_module) = self.modules.get(&module) else {
            return identities;
        };
        for request in &project_module.local.interface().requests {
            let ModuleRequestRole::Import { bindings } = &request.role else {
                continue;
            };
            for binding in bindings {
                if binding.namespace {
                    continue;
                }
                let Some(export) = &binding.imported else {
                    continue;
                };
                let identity = self.resolve_imported_identity(module, &request.specifier, export);
                identities.insert(
                    (request.specifier.clone(), export.clone()),
                    linked_identity(identity),
                );
            }
        }
        // Namespace members do not have binding-specific entries in the local
        // index. Resolve each authored member on demand from the target export
        // table; retaining a wildcard is intentionally avoided.
        for request in &project_module.local.interface().requests {
            let is_namespace_import = match &request.role {
                ModuleRequestRole::Import { bindings } => {
                    bindings.iter().any(|binding| binding.namespace)
                }
                // CommonJS destructuring and direct aliases are represented by
                // the local resolver as module members, but the request itself
                // has no binding list. A wildcard overlay is the precise
                // project-level equivalent and still requires a proven export
                // for internal modules.
                ModuleRequestRole::Require => true,
                ModuleRequestRole::ReExport { .. }
                | ModuleRequestRole::StarExport
                | ModuleRequestRole::DynamicImport => false,
            };
            if is_namespace_import {
                let prefix = request.specifier.clone();
                let identity = self.resolve_namespace(module, request);
                match identity {
                    ExportResolution::Qualified { module: target, .. } => {
                        for export in self.exported_names(target, &mut BTreeSet::new()) {
                            if let Some(resolved) =
                                self.lookup_export(target, &export, &mut BTreeSet::new())
                            {
                                identities
                                    .insert((prefix.clone(), export), linked_identity(resolved));
                            }
                        }
                    }
                    other => {
                        identities.insert((prefix, "*".into()), linked_identity(other));
                    }
                }
            }
        }
        identities
    }

    fn exported_names(
        &self,
        module: ModuleId,
        visiting: &mut BTreeSet<ModuleId>,
    ) -> BTreeSet<String> {
        if !visiting.insert(module) {
            return BTreeSet::new();
        }
        let Some(project_module) = self.modules.get(&module) else {
            return BTreeSet::new();
        };
        let interface = project_module.local.interface();
        let mut names = interface.exports.keys().cloned().collect::<BTreeSet<_>>();
        for request_index in &interface.star_exports {
            let Some(request) = interface.requests.get(*request_index) else {
                continue;
            };
            let key = self.request_key(module, request);
            if let Some(ResolvedModule::Internal { id, .. }) = self.resolutions.get(&key) {
                names.extend(self.exported_names(*id, visiting));
            }
        }
        names
    }

    fn resolve_namespace(
        &self,
        module: ModuleId,
        request: &module::ModuleRequest,
    ) -> ExportResolution {
        match self.resolutions.get(&self.request_key(module, request)) {
            None => ExportResolution::External {
                module: request.specifier.clone(),
                export: "*".into(),
            },
            Some(ResolvedModule::External { package }) => ExportResolution::External {
                module: package.clone(),
                export: "*".into(),
            },
            Some(ResolvedModule::Builtin { name }) => ExportResolution::External {
                module: name.clone(),
                export: "*".into(),
            },
            Some(ResolvedModule::Internal { id, .. }) => ExportResolution::Qualified {
                module: *id,
                export: "*".into(),
            },
            Some(_) => ExportResolution::Unknown,
        }
    }
}

fn linked_identity(resolution: ExportResolution) -> matching::LinkedModuleIdentity {
    match resolution {
        ExportResolution::External { module, export } => {
            matching::LinkedModuleIdentity::External { module, export }
        }
        ExportResolution::Global { name } => matching::LinkedModuleIdentity::Global { name },
        ExportResolution::Qualified { .. } => matching::LinkedModuleIdentity::Qualified,
        ExportResolution::Unknown | ExportResolution::Ambiguous => {
            matching::LinkedModuleIdentity::Unknown
        }
    }
}

fn is_internal_request(request: &str) -> bool {
    request.starts_with('.') || request.starts_with('/') || request.starts_with('#')
}

fn strongly_connected_components(
    adjacency: &BTreeMap<ModuleId, Vec<ModuleId>>,
    nodes: impl IntoIterator<Item = ModuleId>,
) -> Vec<Vec<ModuleId>> {
    // A deterministic Kosaraju pass is sufficient here; the graph is already
    // sorted and deduplicated by the linker.
    fn visit(
        node: ModuleId,
        graph: &BTreeMap<ModuleId, Vec<ModuleId>>,
        seen: &mut BTreeMap<ModuleId, bool>,
        order: &mut Vec<ModuleId>,
    ) {
        if seen.insert(node, true).is_some() {
            return;
        }
        for next in graph.get(&node).into_iter().flatten().copied() {
            visit(next, graph, seen, order);
        }
        order.push(node);
    }
    fn collect(
        node: ModuleId,
        reverse: &BTreeMap<ModuleId, Vec<ModuleId>>,
        seen: &mut BTreeMap<ModuleId, bool>,
        component: &mut Vec<ModuleId>,
    ) {
        if seen.insert(node, true).is_some() {
            return;
        }
        component.push(node);
        for next in reverse.get(&node).into_iter().flatten().copied() {
            collect(next, reverse, seen, component);
        }
    }
    let nodes = nodes.into_iter().collect::<Vec<_>>();
    let mut seen = BTreeMap::new();
    let mut order = Vec::new();
    for node in nodes.iter().copied() {
        visit(node, adjacency, &mut seen, &mut order);
    }
    let mut reverse = adjacency.iter().fold(
        BTreeMap::<ModuleId, Vec<ModuleId>>::new(),
        |mut reverse, (from, tos)| {
            for to in tos {
                reverse.entry(*to).or_default().push(*from);
            }
            reverse
        },
    );
    for values in reverse.values_mut() {
        values.sort_unstable();
    }
    seen.clear();
    let mut components = Vec::new();
    for node in order.into_iter().rev() {
        let mut component = Vec::new();
        collect(node, &reverse, &mut seen, &mut component);
        if !component.is_empty() {
            component.sort_unstable();
            components.push(component);
        }
    }
    components.sort();
    components
}

/// Matcher-specific evidence projected from the completed project model.
#[derive(Debug)]
pub(crate) struct ProjectMatcherModel<'matchers> {
    matchers: CompiledMatcherCatalog<'matchers>,
    projections: BTreeMap<ModuleId, ProjectModuleProjection>,
}

#[derive(Debug)]
struct ProjectModuleProjection {
    index: MatcherFacts,
    arguments: Vec<Vec<ApiEvidence>>,
}

impl ProjectMatcherModel<'_> {
    pub(crate) fn evidence_for(
        &self,
        module: &ProjectModule,
        rule_index: usize,
    ) -> Vec<ApiEvidence> {
        if !self.matchers.is_selected(rule_index) {
            return Vec::new();
        }
        let Some(matcher) = self.matchers.get(rule_index) else {
            return Vec::new();
        };
        let mut evidence = self
            .projections
            .get(&module.id)
            .map_or_else(Vec::new, |projection| {
                projection.index.evidence_for(&matcher.matcher)
            });
        if let Some(projected) = self
            .projections
            .get(&module.id)
            .and_then(|projection| projection.arguments.get(rule_index))
        {
            evidence.extend_from_slice(projected);
        }
        evidence::AnnotatedEvidence::from_evidence(evidence).into_evidence()
    }
}

pub(crate) fn classify_project(
    project: &ProjectSemanticModel,
    catalog: &crate::api::compiler::CompiledCatalog,
    rules: &[crate::api::rule::ApiRule],
    selected: &std::collections::BTreeSet<usize>,
) -> BTreeMap<ModuleId, crate::api::classification::ApiClassificationResult> {
    let matcher_catalog = project.project(catalog.to_matcher_catalog(selected));
    project
        .modules()
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
            (module.id, result)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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
            project.modules().next().expect("one module").local.facts
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
            project.modules().next().expect("one module").local.facts
        );
        assert_eq!(before, after);
    }
}
