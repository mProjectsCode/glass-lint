//! Imported, namespace, and call-result identity overlays.
//!
//! These maps are a qualified overlay consumed during matcher projection.
//! They preserve local value arenas while connecting only identities proven
//! by the export table or by a bounded function-return summary.

use std::collections::{BTreeMap, BTreeSet};

use smol_str::SmolStr;

use crate::analysis::{
    ExportResolution, LinkedModuleTarget, ModuleId, ProjectSemanticModel, QualifiedRequestId,
    matching::{ModuleExportKey, ModuleIdentityMap},
    module::{ImportedBinding, ModuleRequest, ModuleRequestRole},
    project::{
        model::MAX_EXPORT_DEPTH, resolver::target_to_export_resolution, state::LinkingSession,
    },
    syntax::SymbolCallProvenance,
};

impl ProjectSemanticModel {
    /// Connect known function-call results to identities returned by the
    /// qualified target's effect summary.
    pub(super) fn call_result_identities(
        &self,
        importer: ModuleId,
        session: &mut LinkingSession,
    ) -> BTreeMap<crate::analysis::value::ValueId, ExportResolution> {
        let mut identities = BTreeMap::new();
        let Some(module) = self.modules.get(&importer) else {
            return identities;
        };
        let stream = module.local().facts().stream();
        for effect in module.local().effects().iter_effects() {
            for call in effect.calls() {
                let cref = call.as_ref(stream);
                let Some(provenance) = cref.provenance() else {
                    continue;
                };
                let Some((target_module, target_function)) =
                    self.qualified_function_target(importer, cref.target(), provenance, session)
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
                if target.is_invalid() {
                    continue;
                }
                let mut resolution: Option<ExportResolution> = None;
                let mut conflict = false;
                for returned in target.returns() {
                    if returned.parameter().is_some() {
                        continue;
                    }
                    let r = match returned.provenance() {
                        SymbolCallProvenance::ModuleExport { module, export } => {
                            self.resolve_imported_identity(target_module, module, export, session)
                        }
                        SymbolCallProvenance::Global { name } => {
                            ExportResolution::Global { name: name.clone() }
                        }
                        SymbolCallProvenance::Local => self
                            .module_fact_stream(target_module)
                            .and_then(|stream| stream.values().static_string(returned.value()))
                            .map_or(ExportResolution::Unknown, |value| {
                                ExportResolution::StaticString {
                                    value: value.to_owned(),
                                }
                            }),
                        SymbolCallProvenance::Unknown(_) => ExportResolution::Unknown,
                    };
                    match resolution {
                        None => resolution = Some(r),
                        Some(ref prev) if prev != &r => {
                            conflict = true;
                        }
                        _ => {}
                    }
                }
                let resolution = match resolution {
                    Some(r) if !conflict => r,
                    _ => ExportResolution::Unknown,
                };
                identities.insert(cref.result(), resolution);
            }
        }
        identities
    }

    /// Build imported and namespace-member identities for one module.
    pub(super) fn module_identities(
        &self,
        module: ModuleId,
        session: &mut LinkingSession,
    ) -> ModuleIdentityMap {
        let mut identities = ModuleIdentityMap::new();
        let Some(project_module) = self.modules.get(&module) else {
            return identities;
        };
        for request in project_module.local().interface().requests() {
            let is_namespace = match request.role() {
                ModuleRequestRole::Import { bindings } => {
                    for binding in bindings {
                        if binding.is_namespace() {
                            continue;
                        }
                        let Some(export) = binding.imported() else {
                            continue;
                        };
                        let identity = self.resolve_imported_identity(
                            module,
                            request.specifier(),
                            export,
                            session,
                        );
                        identities.insert(
                            ModuleExportKey::new(request.specifier().clone(), export.clone()),
                            identity,
                        );
                    }
                    bindings.iter().any(ImportedBinding::is_namespace)
                }
                ModuleRequestRole::Require | ModuleRequestRole::DynamicImport => true,
                ModuleRequestRole::ReExport { .. } | ModuleRequestRole::StarExport => false,
            };
            if !is_namespace {
                continue;
            }
            let prefix = request.specifier().to_owned();
            match self.resolve_namespace(module, request) {
                ExportResolution::Qualified { module: target, .. } => {
                    self.collect_exported_identities(
                        target,
                        &prefix,
                        &mut BTreeSet::new(),
                        &mut identities,
                    );
                }
                other => {
                    identities.insert(ModuleExportKey::wildcard(prefix), other);
                }
            }
        }
        identities
    }

    /// Walk the resolved export table and star-export chains, collecting
    /// member identities directly into the identity map. Direct exports from
    /// the exporting module are authoritative. Star-exported names from
    /// different sources that disagree are marked ambiguous rather than
    /// allowing the last visited child to silently overwrite earlier ones.
    fn collect_exported_identities(
        &self,
        module: ModuleId,
        prefix: &SmolStr,
        visiting: &mut BTreeSet<ModuleId>,
        identities: &mut ModuleIdentityMap,
    ) {
        if visiting.len() >= MAX_EXPORT_DEPTH || !visiting.insert(module) {
            return;
        }

        // Collect star-exported entries first into a temp map with
        // star-vs-star conflict detection, so that conflicting star-derived
        // names are marked Ambiguous before direct exports are considered.
        let mut star_entries = ModuleIdentityMap::new();
        if let Some(project_module) = self.modules.get(&module) {
            for request_index in project_module.local().interface().star_exports() {
                let Some(request) = project_module.local().interface().request(*request_index)
                else {
                    continue;
                };
                let key = QualifiedRequestId::new(module, request.id());
                if let Some(LinkedModuleTarget::Internal { id, .. }) = self.resolutions.get(&key) {
                    let mut child_entries = ModuleIdentityMap::new();
                    self.collect_exported_identities(*id, prefix, visiting, &mut child_entries);
                    star_entries.merge_star_from(child_entries);
                }
            }
        }

        // Insert resolved export-table entries (authoritative direct/named
        // exports) after star exports so they always win.
        if let Some(exports) = self.exports.module_exports(module) {
            for (name, resolved) in exports.iter() {
                identities.insert(
                    ModuleExportKey::new(prefix.clone(), name.clone()),
                    resolved.clone(),
                );
            }
        }

        // Merge star-exported entries, preserving direct exports.
        identities.merge_missing_from(star_entries);

        visiting.remove(&module);
    }

    /// Resolve a namespace request without guessing at unsupported targets.
    fn resolve_namespace(&self, module: ModuleId, request: &ModuleRequest) -> ExportResolution {
        let key = QualifiedRequestId::new(module, request.id());
        target_to_export_resolution(self.resolutions.get(&key), request.specifier(), "*")
    }
}
