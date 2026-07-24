//! SCC-DAG export fixed-point resolution, single-export lookup, and
//! post-link import validation.

use std::collections::BTreeSet;

use smol_str::{SmolStr, ToSmolStr};

use crate::{
    analysis::{
        LinkedModuleTarget, ModuleId,
        module::{self, DEFAULT_EXPORT, ModuleRequestRole, NAMESPACE_EXPORT},
        project::model::{ExportResolution, MAX_EXPORT_DEPTH},
        status::{AnalysisComponent, IncompleteReason, StatusScope},
        syntax::SymbolCallProvenance,
    },
    project::{
        AnalysisDiagnostic, ProjectRelativePath, is_internal_module_request as is_internal_request,
    },
};

impl super::ProjectLinker {
    /// Resolve exports via a topological walk of the SCC DAG.
    ///
    /// Single-node SCCs resolve in one pass because all dependencies belong
    /// to earlier SCCs and are already settled. Multi-node SCCs use a
    /// local fixed-point bounded per cycle.
    pub(super) fn resolve_export_table(&mut self) {
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
                StatusScope::Project,
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
    pub(super) fn validate_imported_exports(&mut self) {
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
                                StatusScope::File(module.path().clone()),
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

        if let Some(resolved) = self.exports.resolve(module, name) {
            return Some(resolved.clone());
        }

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

        if let Some(resolved) = self.exports.resolve(module, name) {
            return Some(resolved.clone());
        }

        let result = if saw_unknown { None } else { candidate };

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
