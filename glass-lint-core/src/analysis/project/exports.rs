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
        module::{DEFAULT_EXPORT, ModuleRequestRole},
        project::model::MAX_EXPORT_DEPTH,
    },
    project::is_internal_module_request as is_internal_request,
};

impl ProjectSemanticModel {

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
        // The semantic provenance format predates qualified request spans and
        // keys imports by authored module/export. If a virtual caller supplies
        // conflicting answers for repeated requests with the same specifier,
        // preserve precision by treating the identity as ambiguous instead of
        // selecting the first source-order request.
        let mut resolved = None;
        for request_id in interface.request_ids_for_specifier(authored_module) {
            let Some(request) = interface.request(request_id) else {
                continue;
            };
            if !matches!(
                request.role(),
                ModuleRequestRole::Import { .. } | ModuleRequestRole::Require
            ) {
                continue;
            }
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
        // No matching requests: bare package with no resolver answer is
        // intentionally opaque external provenance.
        resolved.unwrap_or_else(|| ExportResolution::External {
            module: authored_module.clone(),
            export: authored_export.clone(),
        })
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

        // Export table is the authoritative source. Check it first so that
        // cache entries never go stale during cycle fixed-point resolution.
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
                Some(LinkedModuleTarget::External { package }) => {
                    Some(ExportResolution::External {
                        module: package.to_smolstr(),
                        export: name.clone(),
                    })
                }
                Some(LinkedModuleTarget::Builtin { name: builtin }) => {
                    Some(ExportResolution::External {
                        module: builtin.to_smolstr(),
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
        // resolution via fixed-point iteration during linking.
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

}
