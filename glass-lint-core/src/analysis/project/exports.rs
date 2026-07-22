//! Export fixed-point resolution and qualified export lookup.
//!
//! Lookups are keyed by the authored request and exact source range. This is
//! important when one module imports the same specifier more than once with
//! different resolver answers; conflicting candidates become unknown rather
//! than inheriting whichever request happens to be visited first.

use smol_str::{SmolStr, ToSmolStr};

use crate::{
    analysis::{
        BTreeSet, ExportResolution, LinkedModuleTarget, ModuleId, ProjectSemanticModel,
        QualifiedRequestId, module,
        module::{DEFAULT_EXPORT, ModuleRequestRole, NAMESPACE_EXPORT},
        project::model::MAX_EXPORT_DEPTH,
        syntax::SymbolCallProvenance,
    },
    project::is_internal_module_request as is_internal_request,
};

impl ProjectSemanticModel {
    /// Resolve one local export into external, qualified, or conservative
    /// unknown identity without merging the exporting module's local scope.
    pub(in crate::analysis) fn resolve_export(
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
            module::ModuleExport::Value => {
                self.static_export_string(module, export_name).map_or_else(
                    || ExportResolution::Qualified {
                        module,
                        export: export_name.to_smolstr(),
                    },
                    |value| ExportResolution::StaticString { value },
                )
            }
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
                        module: package.to_smolstr(),
                        export: NAMESPACE_EXPORT.into(),
                    },
                    Some(LinkedModuleTarget::Builtin { name }) => ExportResolution::External {
                        module: name.to_smolstr(),
                        export: NAMESPACE_EXPORT.into(),
                    },
                    _ => ExportResolution::Unknown,
                }
            }
        }
    }

    /// Resolve an authored module/export pair across all matching requests.
    /// Conflicting request answers are rejected as ambiguous.
    pub(in crate::analysis) fn resolve_imported_identity(
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
            // A bare package with no resolver answer is intentionally opaque
            // external provenance. This preserves isolated-file behavior.
            return ExportResolution::External {
                module: authored_module.clone(),
                export: authored_export.clone(),
            };
        }

        // The semantic provenance format predates qualified request spans and
        // keys imports by authored module/export. If a virtual caller supplies
        // conflicting answers for repeated requests with the same specifier,
        // preserve precision by treating the identity as ambiguous instead of
        // selecting the first source-order request.
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
                    module: package.to_smolstr(),
                    export: authored_export.clone(),
                },
                Some(LinkedModuleTarget::Builtin { name }) => ExportResolution::External {
                    module: name.to_smolstr(),
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
    pub(in crate::analysis) fn request_id(
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
    pub(in crate::analysis) fn lookup_export(
        &self,
        module: ModuleId,
        name: &SmolStr,
        visiting: &mut std::collections::BTreeSet<(ModuleId, SmolStr)>,
    ) -> Option<ExportResolution> {
        let visit_key = (module, name.clone());

        if visiting.len() >= MAX_EXPORT_DEPTH || !visiting.insert(visit_key.clone()) {
            return None;
        }
        if let Some(resolved) = self.exports.resolve(module, name.clone()) {
            let resolved = resolved.clone();
            visiting.remove(&visit_key);
            return Some(resolved);
        }
        // ECMAScript `export *` intentionally does not forward `default`.
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
                Some(
                    LinkedModuleTarget::External { package }
                    | LinkedModuleTarget::Builtin { name: package },
                ) => Some(ExportResolution::External {
                    module: package.to_smolstr(),
                    export: name.clone(),
                }),
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
        if saw_unknown { None } else { candidate }
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
                .lookup_export(*id, imported, &mut std::collections::BTreeSet::new())
                .unwrap_or(ExportResolution::Unknown),
            Some(LinkedModuleTarget::External { package }) => ExportResolution::External {
                module: package.to_smolstr(),
                export: imported.to_smolstr(),
            },
            Some(LinkedModuleTarget::Builtin { name }) => ExportResolution::External {
                module: name.to_smolstr(),
                export: imported.to_smolstr(),
            },
            _ => ExportResolution::Unknown,
        }
    }
}
