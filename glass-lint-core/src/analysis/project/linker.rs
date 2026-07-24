//! Transient linker state for graph construction, SCC-DAG export resolution,
//! and bounded budget enforcement. Consumed into a final
//! [`ProjectSemanticModel`](super::model::ProjectSemanticModel).
//!
//! Graph construction is the boundary between typed resolver answers and
//! core's linker. Only internal targets become edges; all other outcomes are
//! retained as diagnostics.

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use glass_lint_datastructures::{Budget, BudgetTracker};
use smol_str::{SmolStr, ToSmolStr};

use super::{
    model::{ExportResolution, MAX_EXPORT_DEPTH, MAX_SCC_SIZE},
    state::{ExportLookupCache, ExportTable, ModuleGraph, SccPartition},
};
use crate::{
    analysis::{
        LinkedModuleTarget, ModuleId, ProjectModule, QualifiedRequestId,
        module::{self, DEFAULT_EXPORT, ModuleRequestRole, NAMESPACE_EXPORT},
        status::{
            AnalysisComponent, AnalysisStatus, IncompleteReason, ModuleInterfaceKind, StatusScope,
        },
        syntax::SymbolCallProvenance,
    },
    project::{
        AnalysisDiagnostic, ProjectRelativePath, is_internal_module_request as is_internal_request,
    },
};

// ---------------------------------------------------------------------------
// LinkerOutcome
// ---------------------------------------------------------------------------

/// Compact result extracted from a consumed [`ProjectLinker`].
pub(super) struct LinkerOutcome {
    pub modules: BTreeMap<ModuleId, ProjectModule>,
    pub resolutions: BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
    pub edge_count: usize,
    pub exports: ExportTable,
    pub link_cycle_rounds: usize,
    pub diagnostics: Vec<AnalysisDiagnostic>,
    pub status: AnalysisStatus,
}

// ---------------------------------------------------------------------------
// ProjectLinker
// ---------------------------------------------------------------------------

/// Transient linker that owns the module graph, SCC partition, mutable export
/// table, budgets, diagnostics, modules, and resolutions. Consumed into a
/// [`LinkerOutcome`](super::model::ProjectSemanticModel).
pub(super) struct ProjectLinker {
    modules: BTreeMap<ModuleId, ProjectModule>,
    resolutions: BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
    graph: ModuleGraph,
    scc_partition: SccPartition,
    exports: ExportTable,
    lookup_cache: RefCell<ExportLookupCache>,
    link_budget: BudgetTracker,
    link_limit: usize,
    link_cycle_rounds: usize,
    diagnostics: Vec<AnalysisDiagnostic>,
    status: AnalysisStatus,
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
            scc_partition: SccPartition {
                components: Vec::new(),
                dag: BTreeMap::new(),
                order: Vec::new(),
            },
            exports: ExportTable::default(),
            lookup_cache: RefCell::new(ExportLookupCache::new(link_limit)),
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
                    StatusScope::File(path),
                    IncompleteReason::UnsupportedModuleInterface {
                        kind: ModuleInterfaceKind::CommonJsExports,
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
        self.diagnostics.sort_by(|left, right| {
            (
                left.code(),
                left.location().map(crate::project::SourceLocation::path),
                left.location().map(crate::project::SourceLocation::range),
            )
                .cmp(&(
                    right.code(),
                    right.location().map(crate::project::SourceLocation::path),
                    right.location().map(crate::project::SourceLocation::range),
                ))
        });
        self.diagnostics.dedup();
    }

    /// Consume the linker and produce the compact outcome.
    pub(super) fn finish(self) -> LinkerOutcome {
        LinkerOutcome {
            modules: self.modules,
            resolutions: self.resolutions,
            edge_count: self.graph.edge_count(),
            exports: self.exports,
            link_cycle_rounds: self.link_cycle_rounds,
            diagnostics: self.diagnostics,
            status: self.status,
        }
    }

    /// Resolve exports via a topological walk of the SCC DAG.
    ///
    /// Single-node SCCs resolve in one pass because all dependencies belong
    /// to earlier SCCs and are already settled. Multi-node SCCs use a
    /// local fixed-point bounded per cycle.
    fn resolve_export_table(&mut self) {
        let order = self.scc_partition.order.clone();
        if order.is_empty() {
            self.link_cycle_rounds = 0;
            return;
        }

        let components: Vec<Vec<ModuleId>> = self.scc_partition.components.clone();
        let mut total_cycle_rounds = 0usize;

        for &scc_idx in &order {
            let scc = &components[scc_idx];

            if scc.len() == 1 {
                self.resolve_single(scc[0]);
            } else {
                total_cycle_rounds += self.resolve_cycle(scc);
            }
        }

        self.link_cycle_rounds = total_cycle_rounds;

        if self.link_budget.is_exhausted() {
            self.status.record(
                crate::analysis::status::StatusScope::Project,
                IncompleteReason::BudgetExhausted {
                    component: AnalysisComponent::Linking,
                    limit: self.link_limit,
                    observed: Some(self.exports.len()),
                },
            );
        }
    }

    /// Resolve all exports for a single-node SCC. Dependencies are already
    /// final in the memo table, so one pass suffices.
    fn resolve_single(&mut self, module: ModuleId) {
        let exports: Vec<(SmolStr, module::ModuleExport)> = self
            .modules
            .get(&module)
            .into_iter()
            .flat_map(|m| {
                m.local()
                    .interface()
                    .exports()
                    .map(|(n, e)| (n.clone(), e.clone()))
            })
            .collect();
        for (name, export) in exports {
            self.try_set_export(module, &name, &export);
        }
    }

    /// Resolve exports for a multi-node SCC with a local fixed-point.
    /// Returns the number of rounds executed.
    fn resolve_cycle(&mut self, scc: &[ModuleId]) -> usize {
        // Pre-collect exports per module to avoid borrow conflicts.
        let module_exports: Vec<(ModuleId, Vec<(SmolStr, module::ModuleExport)>)> = scc
            .iter()
            .filter_map(|&module| {
                self.modules.get(&module).map(|m| {
                    (
                        module,
                        m.local()
                            .interface()
                            .exports()
                            .map(|(n, e)| (n.clone(), e.clone()))
                            .collect(),
                    )
                })
            })
            .collect();

        let bound = scc.len().saturating_add(1);
        let mut changed = true;
        let mut rounds = 0;
        while changed && rounds < bound {
            changed = false;
            rounds += 1;
            for (module, exports) in &module_exports {
                for (name, export) in exports {
                    if self.try_set_export(*module, name, export) {
                        changed = true;
                    }
                }
            }
        }
        if changed {
            for (module, exports) in &module_exports {
                for (name, _) in exports {
                    if self.exports.resolve(*module, name).is_some() {
                        self.exports
                            .set_monotone(*module, name, ExportResolution::Unknown);
                    }
                }
            }
            self.link_budget.mark_exhausted();
        }
        rounds
    }

    /// Resolve one export and set it in the memo table under budget control.
    /// Returns true if the value changed.
    fn try_set_export(
        &mut self,
        module: ModuleId,
        name: &SmolStr,
        export: &module::ModuleExport,
    ) -> bool {
        let resolved = self.resolve_export(module, name, export);
        if self.exports.resolve(module, name).is_none() && self.exports.len() >= self.link_limit {
            self.link_budget.mark_exhausted();
            return false;
        }
        self.exports.set_monotone(module, name, resolved)
    }

    /// Diagnose imports whose statically requested named export is absent or
    /// ambiguous after linking.
    fn validate_imported_exports(&mut self) {
        for module in self.modules.values() {
            for request in module.local().interface().requests() {
                let Some(key) = self.request_id(module.id(), request) else {
                    continue;
                };
                let Some(LinkedModuleTarget::Internal { id, .. }) = self.resolutions.get(&key)
                else {
                    continue;
                };
                let ModuleRequestRole::Import { bindings } = request.role() else {
                    continue;
                };
                for binding in bindings.iter().filter(|binding| !binding.is_namespace()) {
                    let Some(imported) = binding.imported() else {
                        continue;
                    };
                    match self.lookup_export(*id, imported, &mut BTreeSet::new()) {
                        Some(ExportResolution::Ambiguous) => {
                            self.status.record(
                                crate::analysis::status::StatusScope::File(module.path().clone()),
                                IncompleteReason::AmbiguousStarExport {
                                    request: imported.to_string(),
                                },
                            );
                        }
                        None => self.diagnostics.push(AnalysisDiagnostic::new(
                            crate::project::types::DiagnosticKind::MissingImportedExport.into(),
                            format!("module does not export `{imported}`"),
                            self.modules.get(&module.id()).and_then(|module| {
                                Some(crate::project::SourceLocation::new(
                                    ProjectRelativePath::from_normalized(module.path().to_string()),
                                    module.source_context().range(request.span()).ok()?,
                                ))
                            }),
                        )),
                        Some(_) => {}
                    }
                }
            }
        }
    }

    /// Convert internal resolution records into bounded graph edges, compute
    /// SCCs, build the SCC DAG, and compute the topological order.
    fn collect_graph_edges(&mut self) {
        let mut edge_budget = Budget::new(self.link_limit);
        for module in self.modules.values() {
            self.graph.ensure_node(module.id());
            for request in module.local().interface().requests() {
                let Some(request_id) = self.request_id(module.id(), request) else {
                    continue;
                };
                let Some(resolution) = self.resolutions.get(&request_id) else {
                    if is_internal_request(request.specifier()) {
                        self.status.record(
                            crate::analysis::status::StatusScope::File(module.path().clone()),
                            IncompleteReason::MissingInternalResolution {
                                request: request.specifier().to_string(),
                            },
                        );
                    }
                    continue;
                };
                if let LinkedModuleTarget::Internal { id, .. } = resolution {
                    if edge_budget.try_push() {
                        self.graph.insert_edge(module.id(), *id);
                    } else {
                        self.link_budget.mark_exhausted();
                    }
                } else if matches!(resolution, LinkedModuleTarget::Missing)
                    && is_internal_request(request.specifier())
                {
                    self.status.record(
                        crate::analysis::status::StatusScope::File(module.path().clone()),
                        IncompleteReason::MissingInternalResolution {
                            request: request.specifier().to_string(),
                        },
                    );
                } else if matches!(resolution, LinkedModuleTarget::OutsideProject { .. }) {
                    self.status.record(
                        crate::analysis::status::StatusScope::File(module.path().clone()),
                        IncompleteReason::UnsupportedResolution {
                            request: request.specifier().to_string(),
                            kind: crate::analysis::status::ResolutionKind::OutsideProject,
                        },
                    );
                } else if matches!(resolution, LinkedModuleTarget::Unsupported { .. }) {
                    self.status.record(
                        crate::analysis::status::StatusScope::File(module.path().clone()),
                        IncompleteReason::UnsupportedResolution {
                            request: request.specifier().to_string(),
                            kind: crate::analysis::status::ResolutionKind::Unsupported,
                        },
                    );
                }
            }
        }
        if edge_budget.exhausted() {
            self.link_budget.mark_exhausted();
        }
        self.graph.normalize();

        let components =
            strongly_connected_components(self.graph.forward(), self.modules.keys().copied());

        let oversized = components.iter().any(|c| c.len() > MAX_SCC_SIZE);
        if oversized {
            self.link_budget.mark_exhausted();
            self.scc_partition = SccPartition {
                components,
                dag: BTreeMap::new(),
                order: Vec::new(),
            };
            return;
        }

        let (dag, order) = build_scc_dag_and_order(self.graph.forward(), &components);
        self.scc_partition = SccPartition {
            components,
            dag,
            order,
        };
    }

    // -----------------------------------------------------------------------
    // Export resolution helpers (shared with the final model)
    // -----------------------------------------------------------------------

    /// Resolve one local export into external, qualified, or conservative
    /// unknown identity without merging the exporting module's local scope.
    fn resolve_export(
        &self,
        module: ModuleId,
        export_name: &SmolStr,
        export: &module::ModuleExport,
    ) -> ExportResolution {
        match export {
            module::ModuleExport::Local { name } => {
                let Some(project_module) = self.modules.get(&module) else {
                    return ExportResolution::Unknown;
                };
                if !project_module.local().interface().is_local(name)
                    && project_module.local().export_origin(name).is_none()
                {
                    return ExportResolution::Unknown;
                }
                match project_module.local().export_origin(name) {
                    Some(SymbolCallProvenance::ModuleExport {
                        module: authored_module,
                        export: authored_export,
                    }) => self.resolve_imported_identity(module, authored_module, authored_export),
                    Some(SymbolCallProvenance::Global { name }) => {
                        ExportResolution::Global { name: name.clone() }
                    }
                    Some(SymbolCallProvenance::Local | SymbolCallProvenance::Unknown(_)) | None => {
                        project_module
                            .local()
                            .interface()
                            .static_string(name)
                            .map_or_else(
                                || ExportResolution::Qualified {
                                    module,
                                    export: name.to_smolstr(),
                                },
                                |value| ExportResolution::StaticString {
                                    value: value.clone(),
                                },
                            )
                    }
                }
            }
            module::ModuleExport::Value => self
                .modules
                .get(&module)
                .and_then(|m| m.local().interface().static_string(export_name))
                .cloned()
                .map_or_else(
                    || ExportResolution::Qualified {
                        module,
                        export: export_name.to_smolstr(),
                    },
                    |value| ExportResolution::StaticString { value },
                ),
            module::ModuleExport::Unknown => ExportResolution::Unknown,
            module::ModuleExport::ReExport { request, imported } => {
                self.resolve_request_export(module, *request, imported)
            }
            module::ModuleExport::Namespace { request } => {
                let Some(request) = self
                    .modules
                    .get(&module)
                    .and_then(|m| m.local().interface().request(*request))
                else {
                    return ExportResolution::Unknown;
                };
                let Some(key) = self.request_id(module, request) else {
                    return ExportResolution::Unknown;
                };
                match self.resolutions.get(&key) {
                    Some(LinkedModuleTarget::Internal { id, .. }) => ExportResolution::Qualified {
                        module: *id,
                        export: NAMESPACE_EXPORT.into(),
                    },
                    Some(LinkedModuleTarget::External { package }) => ExportResolution::External {
                        module: package.as_str().to_smolstr(),
                        export: NAMESPACE_EXPORT.into(),
                    },
                    Some(LinkedModuleTarget::Builtin { name }) => ExportResolution::External {
                        module: name.as_str().to_smolstr(),
                        export: NAMESPACE_EXPORT.into(),
                    },
                    _ => ExportResolution::Unknown,
                }
            }
        }
    }

    /// Resolve an authored module/export pair across all matching requests.
    /// Conflicting request answers are rejected as ambiguous.
    fn resolve_imported_identity(
        &self,
        importer: ModuleId,
        authored_module: &SmolStr,
        authored_export: &SmolStr,
    ) -> ExportResolution {
        let Some(interface) = self
            .modules
            .get(&importer)
            .map(|module| module.local().interface())
        else {
            return ExportResolution::Unknown;
        };
        let requests = interface
            .request_ids_for_specifier(authored_module)
            .filter_map(|request| interface.request(request))
            .filter(|request| {
                matches!(
                    request.role(),
                    ModuleRequestRole::Import { .. } | ModuleRequestRole::Require
                )
            })
            .collect::<Vec<_>>();
        if requests.is_empty() {
            return ExportResolution::External {
                module: authored_module.clone(),
                export: authored_export.clone(),
            };
        }

        let mut resolved = None;
        for request in requests {
            let Some(key) = self.request_id(importer, request) else {
                return ExportResolution::Unknown;
            };
            let candidate = match self.resolutions.get(&key) {
                None if is_internal_request(authored_module) => ExportResolution::Unknown,
                None => ExportResolution::External {
                    module: authored_module.clone(),
                    export: authored_export.clone(),
                },
                Some(LinkedModuleTarget::External { package }) => ExportResolution::External {
                    module: package.as_str().to_smolstr(),
                    export: authored_export.clone(),
                },
                Some(LinkedModuleTarget::Builtin { name }) => ExportResolution::External {
                    module: name.as_str().to_smolstr(),
                    export: authored_export.clone(),
                },
                Some(LinkedModuleTarget::Internal { id, .. }) => self
                    .lookup_export(*id, authored_export, &mut BTreeSet::new())
                    .unwrap_or(ExportResolution::Unknown),
                Some(
                    LinkedModuleTarget::Missing
                    | LinkedModuleTarget::OutsideProject { .. }
                    | LinkedModuleTarget::Unsupported { .. },
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

    /// Return the stable internal identity for one local request.
    fn request_id(
        &self,
        module: ModuleId,
        request: &module::ModuleRequest,
    ) -> Option<QualifiedRequestId> {
        self.modules.get(&module)?;
        Some(QualifiedRequestId {
            module,
            request: request.id(),
        })
    }

    /// Resolve an export through direct and star re-exports with cycle bounds.
    /// Results are memoized in `lookup_cache` for O(1) on repeated queries.
    /// The authoritative export table is always checked first so that cache
    /// entries never stale during cycle fixed-point resolution.
    fn lookup_export(
        &self,
        module: ModuleId,
        name: &SmolStr,
        visiting: &mut BTreeSet<(ModuleId, SmolStr)>,
    ) -> Option<ExportResolution> {
        let visit_key = (module, name.clone());

        // Export table is the monotonic authoritative source. Check it first
        // so that cache entries never go stale during cycle resolution.
        if let Some(resolved) = self.exports.resolve(module, name) {
            return Some(resolved.clone());
        }

        // Memoization cache avoids redundant star-export walks for repeated
        // lookups that were not in the export table at resolution time.
        if let Some(cached) = self.lookup_cache.borrow().get(module, name) {
            return cached.clone();
        }

        if visiting.len() >= MAX_EXPORT_DEPTH || !visiting.insert(visit_key.clone()) {
            return None;
        }
        if name == DEFAULT_EXPORT {
            visiting.remove(&visit_key);
            return None;
        }
        let interface = self.modules.get(&module).map(|m| m.local().interface())?;
        if interface.is_unknown() {
            return Some(ExportResolution::Unknown);
        }
        let mut candidate = None;
        let mut saw_unknown = false;
        for request_index in interface.star_exports() {
            let Some(request) = interface.request(*request_index) else {
                saw_unknown = true;
                continue;
            };
            let Some(key) = self.request_id(module, request) else {
                saw_unknown = true;
                continue;
            };
            let resolution = self.resolutions.get(&key);
            let candidate_export = match resolution {
                Some(LinkedModuleTarget::Internal { id, .. }) => {
                    self.lookup_export(*id, name, visiting)
                }
                Some(LinkedModuleTarget::External { package }) => {
                    Some(ExportResolution::External {
                        module: package.as_str().to_smolstr(),
                        export: name.clone(),
                    })
                }
                Some(LinkedModuleTarget::Builtin { name: builtin_name }) => {
                    Some(ExportResolution::External {
                        module: builtin_name.as_str().to_smolstr(),
                        export: name.clone(),
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
        visiting.remove(&visit_key);

        // Re-check export table: the star-export walk may have triggered
        // resolution of the same entry via fixed-point iteration.
        if let Some(resolved) = self.exports.resolve(module, name) {
            return Some(resolved.clone());
        }

        let result = if saw_unknown { None } else { candidate };

        // Populate cache so subsequent lookups for the same key are O(1).
        self.lookup_cache
            .borrow_mut()
            .insert(module, name.clone(), result.clone());

        result
    }

    /// Resolve a named re-export through its authored request.
    fn resolve_request_export(
        &self,
        module: ModuleId,
        request_index: module::ModuleRequestId,
        imported: &SmolStr,
    ) -> ExportResolution {
        let Some(request) = self
            .modules
            .get(&module)
            .and_then(|m| m.local().interface().request(request_index))
        else {
            return ExportResolution::Unknown;
        };
        let Some(key) = self.request_id(module, request) else {
            return ExportResolution::Unknown;
        };
        match self.resolutions.get(&key) {
            Some(LinkedModuleTarget::Internal { id, .. }) => self
                .lookup_export(*id, imported, &mut BTreeSet::new())
                .unwrap_or(ExportResolution::Unknown),
            Some(LinkedModuleTarget::External { package }) => ExportResolution::External {
                module: package.as_str().to_smolstr(),
                export: imported.to_smolstr(),
            },
            Some(LinkedModuleTarget::Builtin { name }) => ExportResolution::External {
                module: name.as_str().to_smolstr(),
                export: imported.to_smolstr(),
            },
            _ => ExportResolution::Unknown,
        }
    }
}

// ---------------------------------------------------------------------------
// SCC and DAG construction (free functions)
// ---------------------------------------------------------------------------

/// Strongly connected components via deterministic iterative Kosaraju.
fn strongly_connected_components(
    adjacency: &BTreeMap<ModuleId, Vec<ModuleId>>,
    nodes: impl IntoIterator<Item = ModuleId>,
) -> Vec<Vec<ModuleId>> {
    let nodes = nodes.into_iter().collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    for node in nodes.iter().copied() {
        if seen.contains(&node) {
            continue;
        }
        let mut stack = vec![(node, false)];
        while let Some((current, expanded)) = stack.pop() {
            if expanded {
                order.push(current);
                continue;
            }
            if !seen.insert(current) {
                continue;
            }
            stack.push((current, true));
            for next in adjacency.get(&current).into_iter().flatten().rev().copied() {
                if !seen.contains(&next) {
                    stack.push((next, false));
                }
            }
        }
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
        if seen.contains(&node) {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![node];
        seen.insert(node);
        while let Some(current) = stack.pop() {
            component.push(current);
            for next in reverse.get(&current).into_iter().flatten().rev().copied() {
                if seen.insert(next) {
                    stack.push(next);
                }
            }
        }
        if !component.is_empty() {
            component.sort_unstable();
            components.push(component);
        }
    }
    components.sort();
    components
}

/// Build the SCC DAG and topological order from the original graph edges and
/// component decomposition.
fn build_scc_dag_and_order(
    forward: &BTreeMap<ModuleId, Vec<ModuleId>>,
    components: &[Vec<ModuleId>],
) -> (BTreeMap<usize, Vec<usize>>, Vec<usize>) {
    let module_to_scc: BTreeMap<ModuleId, usize> = components
        .iter()
        .enumerate()
        .flat_map(|(idx, component)| component.iter().map(move |&m| (m, idx)))
        .collect();

    let mut dag: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (from, targets) in forward {
        let Some(&from_scc) = module_to_scc.get(from) else {
            continue;
        };
        for to in targets {
            let Some(&to_scc) = module_to_scc.get(to) else {
                continue;
            };
            if from_scc != to_scc {
                let edges = dag.entry(from_scc).or_default();
                if !edges.contains(&to_scc) {
                    edges.push(to_scc);
                }
            }
        }
    }
    for edges in dag.values_mut() {
        edges.sort_unstable();
    }

    let scc_count = components.len();
    let mut in_degree = vec![0usize; scc_count];
    for targets in dag.values() {
        for &target in targets {
            in_degree[target] = in_degree[target].saturating_add(1);
        }
    }

    let mut queue: Vec<usize> = (0..scc_count).filter(|&i| in_degree[i] == 0).collect();
    let mut order = Vec::with_capacity(scc_count);
    while let Some(scc_idx) = queue.pop() {
        order.push(scc_idx);
        if let Some(targets) = dag.get(&scc_idx) {
            for &target in targets {
                in_degree[target] = in_degree[target].saturating_sub(1);
                if in_degree[target] == 0 {
                    queue.push(target);
                }
            }
        }
    }

    order.reverse();
    (dag, order)
}
