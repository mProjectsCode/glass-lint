//! Export fixed-point resolution and qualified export lookup.
//!
//! Lookups are keyed by the authored request and exact source range. This is
//! important when one module imports the same specifier more than once with
//! different resolver answers; conflicting candidates become unknown rather
//! than inheriting whichever request happens to be visited first.

use super::super::{
    BTreeSet, ExportResolution, MAX_EXPORT_DEPTH, ModuleId, ProjectSemanticModel,
    ResolutionRequestKey, ResolvedModule, SymbolCallProvenance, module,
};
use crate::{
    analysis::module::ModuleRequestRole, project::is_internal_module_request as is_internal_request,
};

impl ProjectSemanticModel {
    /// Resolve one local export into external, qualified, or conservative
    /// unknown identity without merging the exporting module's local scope.
    pub(in crate::analysis) fn resolve_export(
        &self,
        module: ModuleId,
        export_name: &str,
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
                    Some(SymbolCallProvenance::Local) | None => project_module
                        .local()
                        .interface()
                        .static_string(name)
                        .map_or_else(
                            || ExportResolution::Qualified {
                                module,
                                export: name.clone(),
                            },
                            |value| ExportResolution::StaticString {
                                value: value.clone(),
                            },
                        ),
                }
            }
            module::ModuleExport::Value => {
                self.static_export_string(module, export_name).map_or_else(
                    || ExportResolution::Qualified {
                        module,
                        export: export_name.to_owned(),
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
                let key = ResolutionRequestKey {
                    importer: self.modules[&module].path().to_owned().into(),
                    kind: request.kind(),
                    range: crate::lint::source_range_from_span(
                        self.modules[&module].source_map(),
                        request.span(),
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

    /// Resolve an authored module/export pair across all matching requests.
    /// Conflicting request answers are rejected as ambiguous.
    pub(in crate::analysis) fn resolve_imported_identity(
        &self,
        importer: ModuleId,
        authored_module: &str,
        authored_export: &str,
    ) -> ExportResolution {
        let requests = self
            .modules
            .get(&importer)
            .into_iter()
            .flat_map(|module| module.local().interface().requests())
            .filter(|request| {
                request.specifier() == authored_module
                    && matches!(
                        request.role(),
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

    /// Build the stable public resolution key for one local request.
    pub(in crate::analysis) fn request_key(
        &self,
        module: ModuleId,
        request: &module::ModuleRequest,
    ) -> ResolutionRequestKey {
        ResolutionRequestKey {
            importer: self.modules[&module].path().to_owned().into(),
            kind: request.kind(),
            range: crate::lint::source_range_from_span(
                self.modules[&module].source_map(),
                request.span(),
            ),
        }
    }

    /// Resolve an export through direct and star re-exports with cycle bounds.
    pub(in crate::analysis) fn lookup_export(
        &self,
        module: ModuleId,
        name: &str,
        visiting: &mut std::collections::BTreeSet<(ModuleId, String)>,
    ) -> Option<ExportResolution> {
        let visit_key = (module, name.to_string());
        if visiting.len() >= MAX_EXPORT_DEPTH || !visiting.insert(visit_key.clone()) {
            return None;
        }
        if let Some(resolved) = self.exports.resolve(module, name) {
            let resolved = resolved.clone();
            visiting.remove(&visit_key);
            return Some(resolved);
        }
        // ECMAScript `export *` intentionally does not forward `default`.
        if name == "default" {
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
            let key = ResolutionRequestKey {
                importer: self.modules[&module].path().to_owned().into(),
                kind: request.kind(),
                range: crate::lint::source_range_from_span(
                    self.modules[&module].source_map(),
                    request.span(),
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
        visiting.remove(&visit_key);
        if saw_unknown { None } else { candidate }
    }

    /// Resolve a named re-export through its authored request.
    fn resolve_request_export(
        &self,
        module: ModuleId,
        request_index: usize,
        imported: &str,
    ) -> ExportResolution {
        let Some(request) = self
            .modules
            .get(&module)
            .and_then(|m| m.local().interface().request(request_index))
        else {
            return ExportResolution::Unknown;
        };
        let key = ResolutionRequestKey {
            importer: self.modules[&module].path().to_owned().into(),
            kind: request.kind(),
            range: crate::lint::source_range_from_span(
                self.modules[&module].source_map(),
                request.span(),
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
}
