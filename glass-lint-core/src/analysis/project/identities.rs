//! Imported, namespace, and call-result identity overlays.
//!
//! These maps are a qualified overlay consumed during matcher projection.
//! They preserve local value arenas while connecting only identities proven
//! by the export table or by a bounded function-return summary.

use std::collections::{BTreeMap, BTreeSet};

use smol_str::{SmolStr, ToSmolStr};

use crate::analysis::{
    ExportResolution, LinkedModuleTarget, ModuleId, ProjectSemanticModel,
    matching::{LinkedModuleIdentity, ModuleExportKey, ModuleIdentityMap},
    module::{ImportedBinding, ModuleRequest, ModuleRequestRole},
    project::model::MAX_EXPORT_DEPTH,
    syntax::SymbolCallProvenance,
};

impl ProjectSemanticModel {
    /// Connect known function-call results to identities returned by the
    /// qualified target's effect summary.
    pub(super) fn call_result_identities(
        &self,
        importer: ModuleId,
    ) -> BTreeMap<crate::analysis::value::ValueId, LinkedModuleIdentity> {
        let mut identities = BTreeMap::new();
        let Some(module) = self.modules.get(&importer) else {
            return identities;
        };
        for effect in module.local().effects().iter_effects() {
            for call in effect.calls() {
                let Some((target_module, target_function)) =
                    self.qualified_function_target(importer, call.target(), call.provenance())
                else {
                    continue;
                };
                let Some(target) = self
                    .modules
                    .get(&target_module)
                    .and_then(|module| module.local().effects().get(target_function))
                else {
                    continue;
                };
                let Some(returned) = target
                    .returns()
                    .iter()
                    .find(|returned| returned.parameter().is_none())
                else {
                    continue;
                };
                let resolution = match returned.provenance() {
                    SymbolCallProvenance::ModuleExport { module, export } => {
                        self.resolve_imported_identity(target_module, module, export)
                    }
                    SymbolCallProvenance::Global { name } => {
                        ExportResolution::Global { name: name.clone() }
                    }
                    SymbolCallProvenance::Local => {
                        returned
                            .static_string()
                            .map_or(ExportResolution::Unknown, |value| {
                                ExportResolution::StaticString {
                                    value: value.to_owned(),
                                }
                            })
                    }
                    SymbolCallProvenance::Unknown(_) => ExportResolution::Unknown,
                };
                identities.insert(call.result(), resolution.into());
            }
        }
        identities
    }

    /// Build imported and namespace-member identities for one module.
    pub(super) fn module_identities(&self, module: ModuleId) -> ModuleIdentityMap {
        let mut identities = BTreeMap::new();
        let Some(project_module) = self.modules.get(&module) else {
            return identities;
        };
        for request in project_module.local().interface().requests() {
            let ModuleRequestRole::Import { bindings } = request.role() else {
                continue;
            };
            for binding in bindings {
                if binding.is_namespace() {
                    continue;
                }
                let Some(export) = binding.imported() else {
                    continue;
                };
                let identity = self.resolve_imported_identity(module, request.specifier(), export);
                identities.insert(
                    ModuleExportKey::new(request.specifier().clone(), export.clone()),
                    identity.into(),
                );
            }
        }
        for request in project_module.local().interface().requests() {
            let is_namespace_import = match request.role() {
                ModuleRequestRole::Import { bindings } => {
                    bindings.iter().any(ImportedBinding::is_namespace)
                }
                ModuleRequestRole::Require | ModuleRequestRole::DynamicImport => true,
                ModuleRequestRole::ReExport { .. } | ModuleRequestRole::StarExport => false,
            };
            if !is_namespace_import {
                continue;
            }
            let prefix = request.specifier().to_owned();
            match self.resolve_namespace(module, request) {
                ExportResolution::Qualified { module: target, .. } => {
                    for export in self.exported_names(target, &mut BTreeSet::new()) {
                        if let Some(resolved) =
                            self.lookup_export(target, &export, &mut BTreeSet::new())
                        {
                            identities.insert(
                                ModuleExportKey::new(prefix.clone(), export),
                                resolved.into(),
                            );
                        }
                    }
                }
                other => {
                    identities.insert(ModuleExportKey::wildcard(prefix), other.into());
                }
            }
        }
        identities
    }

    /// Collect statically named exports reachable through star re-exports.
    fn exported_names(
        &self,
        module: ModuleId,
        visiting: &mut BTreeSet<ModuleId>,
    ) -> BTreeSet<SmolStr> {
        if visiting.len() >= MAX_EXPORT_DEPTH || !visiting.insert(module) {
            return BTreeSet::new();
        }
        let Some(project_module) = self.modules.get(&module) else {
            return BTreeSet::new();
        };
        let interface = project_module.local().interface();
        let mut names = interface
            .exports()
            .map(|(name, _)| name.clone())
            .collect::<BTreeSet<_>>();
        for request_index in interface.star_exports() {
            let Some(request) = interface.request(*request_index) else {
                continue;
            };
            let Some(key) = self.request_id(module, request) else {
                continue;
            };
            if let Some(LinkedModuleTarget::Internal { id, .. }) = self.resolutions.get(&key) {
                names.extend(self.exported_names(*id, visiting));
            }
        }
        visiting.remove(&module);
        names
    }

    /// Resolve a namespace request without guessing at unsupported targets.
    fn resolve_namespace(&self, module: ModuleId, request: &ModuleRequest) -> ExportResolution {
        let Some(key) = self.request_id(module, request) else {
            return ExportResolution::Unknown;
        };
        match self.resolutions.get(&key) {
            None => ExportResolution::External {
                module: request.specifier().clone(),
                export: "*".into(),
            },
            Some(LinkedModuleTarget::External { package }) => ExportResolution::External {
                module: package.to_smolstr(),
                export: "*".into(),
            },
            Some(LinkedModuleTarget::Builtin { name }) => ExportResolution::External {
                module: name.to_smolstr(),
                export: "*".into(),
            },
            Some(LinkedModuleTarget::Internal { id, .. }) => ExportResolution::Qualified {
                module: *id,
                export: "*".into(),
            },
            Some(_) => ExportResolution::Unknown,
        }
    }
}
