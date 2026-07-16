//! Project graph construction and bounded SCC analysis.
//!
//! Graph construction is the boundary between typed resolver answers and
//! core's linker. Only internal targets become edges; all other outcomes are
//! retained as diagnostics or unknown provenance.

use super::super::{
    BTreeMap, ExportResolution, MAX_EXPORT_ENTRIES, MAX_GRAPH_EDGES, MAX_SCC_SIZE, ModuleId,
    ProjectSemanticModel, ResolvedModule,
};
use crate::{
    analysis::module::ModuleRequestRole, project::is_internal_module_request as is_internal_request,
};

impl ProjectSemanticModel {
    /// Build edges, resolve exports, validate imports, and canonicalize
    /// diagnostics in the fixed project-linking order.
    pub(in crate::analysis) fn build_graph_and_exports(&mut self) {
        self.collect_graph_edges();
        self.resolve_export_table();
        self.validate_imported_exports();
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

    /// Compute the bounded monotone export table and record budget failures.
    fn resolve_export_table(&mut self) {
        // Resolve exports in a bounded, monotone pass.  The fixed point is
        // intentionally conservative: a cycle that does not stabilize stays
        // unknown instead of depending on module iteration order.
        let mut changed = true;
        let mut rounds = 0;
        while changed && rounds < self.modules.len().saturating_add(1) {
            changed = false;
            rounds += 1;
            for module in self.modules.values() {
                for (name, export) in module.local().interface().exports() {
                    let resolved = self.resolve_export(module.id(), name, export);
                    if self.exports.resolve(module.id(), name).is_none()
                        && self.exports.len() >= MAX_EXPORT_ENTRIES
                    {
                        self.link_budget.mark_exhausted();
                        continue;
                    }
                    if self.exports.resolve(module.id(), name) != Some(&resolved) {
                        self.exports
                            .set_monotone(module.id(), name.clone(), resolved);
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
            self.exports.mark_unknown();
            self.link_budget.mark_exhausted();
        }
        if self.link_budget.is_exhausted() {
            self.diagnostics.push(crate::ProjectDiagnostic {
                code: "graph_link_budget_exhausted".into(),
                message: "project graph linking exceeded a bounded linker budget".into(),
                location: None,
            });
        }
    }

    /// Diagnose imports whose statically requested named export is absent or
    /// ambiguous after linking.
    fn validate_imported_exports(&mut self) {
        for module in self.modules.values() {
            for request in module.local().interface().requests() {
                let key = crate::project::ResolutionRequestKey {
                    importer: module.path().to_owned().into(),
                    kind: request.kind(),
                    range: crate::lint::source_range_from_span(module.source_map(), request.span()),
                };
                let Some(ResolvedModule::Internal { id, .. }) = self.resolutions.get(&key) else {
                    continue;
                };
                let ModuleRequestRole::Import { bindings } = request.role() else {
                    continue;
                };
                for binding in bindings.iter().filter(|binding| !binding.is_namespace()) {
                    let Some(imported) = binding.imported() else {
                        continue;
                    };
                    match self.lookup_export(*id, imported, &mut std::collections::BTreeSet::new())
                    {
                        Some(ExportResolution::Ambiguous) => {
                            self.diagnostics.push(crate::ProjectDiagnostic {
                                code: "ambiguous_star_export".into(),
                                message: format!("module export `{imported}` is ambiguous"),
                                location: Some(crate::SourceLocation {
                                    path: module.path().to_owned().into(),
                                    range: key.range.clone(),
                                }),
                            });
                        }
                        None => self.diagnostics.push(crate::ProjectDiagnostic {
                            code: "missing_imported_export".into(),
                            message: format!("module does not export `{imported}`"),
                            location: Some(crate::SourceLocation {
                                path: module.path().to_owned().into(),
                                range: key.range.clone(),
                            }),
                        }),
                        Some(_) => {}
                    }
                }
            }
        }
    }

    /// Convert internal resolution records into bounded graph edges and SCCs.
    fn collect_graph_edges(&mut self) {
        let mut edge_budget = crate::budget::Budget::new(MAX_GRAPH_EDGES);
        for module in self.modules.values() {
            self.graph.ensure_node(module.id());
            self.diagnostics.extend(module.diagnostics());
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
                                path: module.path().to_owned().into(),
                                range: request.key.range.clone(),
                            }),
                        });
                    }
                    continue;
                };
                if let ResolvedModule::Internal { id, .. } = resolution {
                    if edge_budget.try_push() {
                        self.graph
                            .insert_edge(module.id(), *id, request.key.clone());
                    } else {
                        self.link_budget.mark_exhausted();
                    }
                } else if matches!(resolution, ResolvedModule::Missing)
                    && is_internal_request(&request.request)
                {
                    self.diagnostics.push(Self::request_diagnostic(
                        "unresolved_internal_request",
                        format!("internal module request `{}` is missing", request.request),
                        module,
                        &request,
                    ));
                } else if matches!(resolution, ResolvedModule::OutsideProject { .. }) {
                    self.diagnostics.push(Self::request_diagnostic(
                        "outside_project_target",
                        format!(
                            "module request `{}` resolves outside the project",
                            request.request
                        ),
                        module,
                        &request,
                    ));
                } else if matches!(resolution, ResolvedModule::Unsupported { .. }) {
                    self.diagnostics.push(Self::request_diagnostic(
                        "unsupported_project_target",
                        format!(
                            "module request `{}` is not an analyzable project target",
                            request.request
                        ),
                        module,
                        &request,
                    ));
                }
            }
        }
        if edge_budget.exhausted() {
            self.link_budget.mark_exhausted();
        }
        self.graph.normalize();
        let components =
            strongly_connected_components(self.graph.forward(), self.modules.keys().copied());
        self.graph.set_components(components);
        if self
            .graph
            .components()
            .iter()
            .any(|component| component.len() > MAX_SCC_SIZE)
        {
            self.diagnostics.push(crate::ProjectDiagnostic {
                code: "scc_size_budget_exhausted".into(),
                message: format!("a module SCC exceeds the bounded size of {MAX_SCC_SIZE}"),
                location: None,
            });
            self.link_budget.mark_exhausted();
        }
    }

    /// Create a source-qualified diagnostic for one rejected request target.
    fn request_diagnostic(
        code: &str,
        message: String,
        module: &super::super::ProjectModule,
        request: &crate::ResolutionRequest,
    ) -> crate::ProjectDiagnostic {
        crate::ProjectDiagnostic {
            code: code.into(),
            message,
            location: Some(crate::SourceLocation {
                path: module.path().to_owned().into(),
                range: request.key.range.clone(),
            }),
        }
    }
}

fn strongly_connected_components(
    adjacency: &BTreeMap<ModuleId, Vec<ModuleId>>,
    nodes: impl IntoIterator<Item = ModuleId>,
) -> Vec<Vec<ModuleId>> {
    // A deterministic iterative Kosaraju pass avoids stack growth from a
    // large explicit project graph while preserving stable module ordering.
    let nodes = nodes.into_iter().collect::<Vec<_>>();
    let mut seen = BTreeMap::new();
    let mut order = Vec::new();
    for node in nodes.iter().copied() {
        if seen.contains_key(&node) {
            continue;
        }
        let mut stack = vec![(node, false)];
        while let Some((current, expanded)) = stack.pop() {
            if expanded {
                order.push(current);
                continue;
            }
            if seen.insert(current, true).is_some() {
                continue;
            }
            stack.push((current, true));
            for next in adjacency.get(&current).into_iter().flatten().rev().copied() {
                if !seen.contains_key(&next) {
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
        if seen.get(&node).copied().unwrap_or(false) {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![node];
        seen.insert(node, true);
        while let Some(current) = stack.pop() {
            component.push(current);
            for next in reverse.get(&current).into_iter().flatten().rev().copied() {
                if let std::collections::btree_map::Entry::Vacant(entry) = seen.entry(next) {
                    entry.insert(true);
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
