//! SCC-DAG export fixed-point resolution, single-export lookup, and
//! post-link import validation.

use smol_str::{SmolStr, ToSmolStr};

use crate::{
    analysis::{
        LinkedModuleTarget, ModuleId, QualifiedRequestId,
        model::module::{self, ModuleRequestRole, NAMESPACE_EXPORT},
        project::{
            linker::ProjectLinker,
            model::ExportResolution,
            resolver::linked_target_to_export_resolution,
            state::{ExportUpdate, QualifiedExportId},
        },
        semantic::status::{AnalysisComponent, IncompleteReason, StatusScope},
        syntax::SymbolCallProvenance,
    },
    project::{AnalysisDiagnostic, ProjectRelativePath},
};

impl ProjectLinker {
    fn module_exports(&self, module: ModuleId) -> Vec<(SmolStr, module::ModuleExport)> {
        self.modules
            .get(&module)
            .into_iter()
            .flat_map(|project_module| {
                project_module
                    .local()
                    .interface()
                    .exports()
                    .map(|(name, export)| (name.clone(), export.clone()))
            })
            .collect()
    }

    /// Resolve exports via a topological walk of the SCC DAG.
    ///
    /// Single-node SCCs resolve in one pass because all dependencies belong
    /// to earlier SCCs and are already settled. Multi-node SCCs use a
    /// local fixed-point bounded per cycle.
    pub(super) fn resolve_export_table(&mut self) {
        let Some(partition) = self.scc_partition.take() else {
            return;
        };
        let mut total_cycle_rounds = 0usize;

        for scc in partition.ordered_components() {
            if scc.len() == 1 {
                self.resolve_single(scc[0]);
            } else {
                total_cycle_rounds += self.resolve_cycle(scc);
            }
        }

        self.link_cycle_rounds = total_cycle_rounds;
        let has_components = !partition.is_empty();
        self.scc_partition = Some(partition);

        if has_components && self.link_budget.is_exhausted() {
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
        let exports = self.module_exports(module);
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
                self.modules
                    .get(&module)
                    .map(|_| (module, self.module_exports(module)))
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
                    let id = QualifiedExportId::new(*module, name.clone());
                    if self.exports.resolve(&id).is_some() {
                        self.exports.set_resolution(id, ExportResolution::Unknown);
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
        let id = QualifiedExportId::new(module, name.clone());
        if self.exports.resolve(&id).is_none() && self.exports.len() >= self.link_limit {
            self.link_budget.mark_exhausted();
            return false;
        }
        !matches!(
            self.exports.set_resolution(id, resolved),
            ExportUpdate::Unchanged
        )
    }

    /// Diagnose imports whose statically requested named export is absent or
    /// ambiguous after linking.
    pub(super) fn validate_imported_exports(&mut self) {
        let mut checks = Vec::new();
        for module in self.modules.values() {
            for (request_id, request) in module.local().interface().request_entries() {
                let key = QualifiedRequestId::new(module.id(), request_id);
                let Some(LinkedModuleTarget::Internal { id }) = self.resolutions.get(&key) else {
                    continue;
                };
                let ModuleRequestRole::Import { bindings } = request.role() else {
                    continue;
                };
                for binding in bindings {
                    let Some(imported) = binding.imported() else {
                        continue;
                    };
                    checks.push((module.id(), *id, request_id, imported.clone()));
                }
            }
        }
        for (importer, target, request_id, imported) in checks {
            match self.lookup_export(&QualifiedExportId::new(target, imported.clone())) {
                Some(ExportResolution::Ambiguous) => {
                    if let Some(module) = self.modules.get(&importer) {
                        self.status.record(
                            StatusScope::File(module.path().clone()),
                            IncompleteReason::AmbiguousStarExport {
                                request: imported.to_string(),
                            },
                        );
                    }
                }
                None => {
                    self.diagnostics.push(AnalysisDiagnostic::new(
                        crate::project::types::DiagnosticKind::MissingImportedExport.into(),
                        format!("module does not export `{imported}`"),
                        self.modules.get(&importer).and_then(|module| {
                            Some(crate::project::SourceLocation::new(
                                ProjectRelativePath::from_normalized(module.path().to_string()),
                                module.local().interface().request(request_id).and_then(
                                    |request| module.source_context().range(request.span()).ok(),
                                )?,
                            ))
                        }),
                    ));
                }
                Some(_) => {}
            }
        }
    }

    // -----------------------------------------------------------------------
    // Export resolution helpers (shared with the final model)
    // -----------------------------------------------------------------------

    /// Resolve one local export into external, qualified, or conservative
    /// unknown identity without merging the exporting module's local scope.
    // Kept as a single match: each export variant follows a distinct resolution
    // path that is clearest when read side by side.
    fn resolve_export(
        &mut self,
        module: ModuleId,
        export_name: &SmolStr,
        export: &module::ModuleExport,
    ) -> ExportResolution {
        match export {
            module::ModuleExport::Local { name } => self.resolve_local_export(module, name),
            module::ModuleExport::Value => self.resolve_value_export(module, export_name),
            module::ModuleExport::Unknown => ExportResolution::Unknown,
            module::ModuleExport::ReExport { request, imported } => {
                self.resolve_request_export(module, *request, imported)
            }
            module::ModuleExport::Namespace { request } => {
                self.resolve_namespace_export(module, *request)
            }
        }
    }

    fn resolve_local_export(&mut self, module: ModuleId, name: &SmolStr) -> ExportResolution {
        let Some(project_module) = self.modules.get(&module) else {
            return ExportResolution::Unknown;
        };
        let is_local = project_module.local().interface().is_local(name);
        let origin = project_module.local().export_origin(name).cloned();
        let static_string = project_module
            .local()
            .interface()
            .static_string(name)
            .map(str::to_owned);
        if !is_local && origin.is_none() {
            return ExportResolution::Unknown;
        }
        match origin {
            Some(SymbolCallProvenance::ModuleExport {
                module: authored_module,
                export: authored_export,
            }) => self.resolve_imported_identity(module, &authored_module, &authored_export),
            Some(SymbolCallProvenance::Global { name }) => ExportResolution::Global { name },
            Some(SymbolCallProvenance::Local | SymbolCallProvenance::Unknown(_)) | None => {
                static_string.map_or_else(
                    || ExportResolution::Qualified {
                        module,
                        export: name.to_smolstr(),
                    },
                    |value| ExportResolution::StaticString { value },
                )
            }
        }
    }

    fn resolve_value_export(&self, module: ModuleId, export_name: &SmolStr) -> ExportResolution {
        self.modules
            .get(&module)
            .and_then(|project_module| {
                project_module
                    .local()
                    .interface()
                    .static_string(export_name)
            })
            .map(str::to_owned)
            .map_or_else(
                || ExportResolution::Qualified {
                    module,
                    export: export_name.to_smolstr(),
                },
                |value| ExportResolution::StaticString { value },
            )
    }

    fn resolve_namespace_export(
        &self,
        module: ModuleId,
        request_index: module::ModuleRequestId,
    ) -> ExportResolution {
        let Some(project_module) = self.modules.get(&module) else {
            return ExportResolution::Unknown;
        };
        if !project_module
            .local()
            .interface()
            .has_request(request_index)
        {
            return ExportResolution::Unknown;
        }
        let key = QualifiedRequestId::new(module, request_index);
        self.resolutions
            .get(&key)
            .map_or(ExportResolution::Unknown, |target| {
                linked_target_to_export_resolution(target, NAMESPACE_EXPORT)
            })
    }

    fn resolve_imported_identity(
        &mut self,
        importer: ModuleId,
        authored_module: &SmolStr,
        authored_export: &SmolStr,
    ) -> ExportResolution {
        self.with_export_resolver(|resolver| {
            resolver.resolve_imported_identity(importer, authored_module, authored_export)
        })
    }

    fn lookup_export(&mut self, id: &QualifiedExportId) -> Option<ExportResolution> {
        self.with_export_resolver(|resolver| resolver.lookup_export(id))
    }

    /// Resolve a named re-export through its authored request.
    fn resolve_request_export(
        &mut self,
        module: ModuleId,
        request_index: module::ModuleRequestId,
        imported: &SmolStr,
    ) -> ExportResolution {
        let Some(project_module) = self.modules.get(&module) else {
            return ExportResolution::Unknown;
        };
        if !project_module
            .local()
            .interface()
            .has_request(request_index)
        {
            return ExportResolution::Unknown;
        }
        self.with_export_resolver(|resolver| {
            resolver
                .resolve_request_target(module, request_index, imported)
                .unwrap_or(ExportResolution::Unknown)
        })
    }
}
