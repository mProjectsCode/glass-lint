//! Project graph construction and bounded SCC analysis.
//!
//! Graph construction is the boundary between typed resolver answers and
//! core's linker. Only internal targets become edges; all other outcomes are
//! retained as diagnostics or unknown provenance.

use crate::{
    analysis::{
        BTreeMap, ExportResolution, LinkedModuleTarget, ModuleId, ProjectSemanticModel,
        module::ModuleRequestRole,
        project::model::MAX_SCC_SIZE,
        status::{AnalysisComponent, IncompleteReason},
    },
    project::{ProjectRelativePath, is_internal_module_request as is_internal_request},
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
                    if self.exports.resolve(module.id(), name.clone()).is_none()
                        && self.exports.len() >= self.link_limit()
                    {
                        self.link_budget.mark_exhausted();
                        continue;
                    }
                    if self.exports.resolve(module.id(), name.clone()) != Some(&resolved) {
                        self.exports
                            .set_monotone(module.id(), name.clone(), resolved);
                        changed = true;
                    }
                }
            }
        }
        self.link_rounds = rounds;
        if changed {
            self.exports.mark_unknown();
            self.link_budget.mark_exhausted();
            self.status.record(
                crate::analysis::status::StatusScope::Project,
                IncompleteReason::BudgetExhausted {
                    component: AnalysisComponent::Linking,
                    limit: self.link_limit(),
                    observed: Some(rounds),
                },
            );
        }
        if self.link_budget.is_exhausted() {
            self.status.record(
                crate::analysis::status::StatusScope::Project,
                IncompleteReason::BudgetExhausted {
                    component: AnalysisComponent::Linking,
                    limit: self.link_limit(),
                    observed: Some(self.exports.len()),
                },
            );
        }
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
                    match self.lookup_export(*id, imported, &mut std::collections::BTreeSet::new())
                    {
                        Some(ExportResolution::Ambiguous) => {
                            self.status.record(
                                crate::analysis::status::StatusScope::File(module.path().clone()),
                                IncompleteReason::AmbiguousStarExport {
                                    request: imported.to_string(),
                                },
                            );
                        }
                        None => self.diagnostics.push(crate::AnalysisDiagnostic {
                            code: crate::project::types::DiagnosticKind::MissingImportedExport
                                .into(),
                            message: format!("module does not export `{imported}`"),
                            location: self.modules.get(&module.id()).and_then(|module| {
                                Some(crate::SourceLocation {
                                    path: ProjectRelativePath::from_normalized(
                                        module.path().to_string(),
                                    ),
                                    range: module.source_context().range(request.span()).ok()?,
                                })
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
        let mut edge_budget = crate::budget::Budget::new(self.link_limit());
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
                        self.graph.insert_edge(module.id(), *id, request_id);
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
        self.graph.set_components(components);
        if self
            .graph
            .components()
            .iter()
            .any(|component| component.len() > MAX_SCC_SIZE)
        {
            self.link_budget.mark_exhausted();
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
