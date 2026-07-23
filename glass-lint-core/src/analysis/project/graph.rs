//! Project graph construction, SCC-DAG export resolution, and bounded
//! budget enforcement.
//!
//! Graph construction is the boundary between typed resolver answers and
//! core's linker. Only internal targets become edges; all other outcomes are
//! retained as diagnostics.

use crate::{
    analysis::{
        BTreeMap, ExportResolution, LinkedModuleTarget, ModuleId, ProjectSemanticModel,
        module::ModuleRequestRole,
        project::{model::MAX_SCC_SIZE, state::SccPartition},
        status::{AnalysisComponent, IncompleteReason},
    },
    project::{ProjectRelativePath, is_internal_module_request as is_internal_request},
};

impl ProjectSemanticModel {
    /// Build edges, resolve exports via SCC-DAG topological walk, validate
    /// imports, and canonicalize diagnostics.
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
                    limit: self.link_limit(),
                    observed: Some(self.exports.len()),
                },
            );
        }
    }

    /// Resolve all exports for a single-node SCC. Dependencies are already
    /// final in the memo table, so one pass suffices.
    fn resolve_single(&mut self, module: ModuleId) {
        let exports: Vec<(smol_str::SmolStr, crate::analysis::module::ModuleExport)> = self
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
        let module_exports: Vec<(
            ModuleId,
            Vec<(smol_str::SmolStr, crate::analysis::module::ModuleExport)>,
        )> = scc
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
            for &(module, ref exports) in &module_exports {
                for (name, export) in exports {
                    if self.try_set_export(module, name, export) {
                        changed = true;
                    }
                }
            }
        }
        if changed {
            for &(module, ref exports) in &module_exports {
                for (name, _) in exports {
                    if self.exports.resolve(module, name).is_some() {
                        self.exports
                            .set_monotone(module, name, ExportResolution::Unknown);
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
        name: &smol_str::SmolStr,
        export: &crate::analysis::module::ModuleExport,
    ) -> bool {
        let resolved = self.resolve_export(module, name, export);
        if self.exports.resolve(module, name).is_none() && self.exports.len() >= self.link_limit() {
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

    /// Convert internal resolution records into bounded graph edges, compute
    /// SCCs, build the SCC DAG, and compute the topological order.
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
}

/// Strongly connected components via deterministic iterative Kosaraju.
fn strongly_connected_components(
    adjacency: &BTreeMap<ModuleId, Vec<ModuleId>>,
    nodes: impl IntoIterator<Item = ModuleId>,
) -> Vec<Vec<ModuleId>> {
    let nodes = nodes.into_iter().collect::<Vec<_>>();
    let mut seen = std::collections::BTreeSet::new();
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

    // Kahn's algorithm for topological sort (dependency-first).
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

    // Forward edges go from importer to dependency, so Kahn's output is
    // importer-first.  Reverse to get dependency-first processing order.
    order.reverse();

    (dag, order)
}
