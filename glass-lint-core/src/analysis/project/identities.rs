//! Imported, namespace, and call-result identity overlays.
//!
//! These maps are a qualified overlay consumed during matcher projection.
//! They preserve local value arenas while connecting only identities proven
//! by the export table or by a bounded function-return summary.

use std::collections::{BTreeMap, BTreeSet};

use smol_str::SmolStr;

use crate::analysis::{
    ExportResolution, LinkedModuleTarget, ModuleId, ProjectSemanticModel, QualifiedRequestId,
    flow::effect::CallEffectRef,
    matching::{ModuleExportKey, ModuleIdentityMap},
    model::module::{ImportedBinding, ModuleRequest, ModuleRequestRole},
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
    ) -> BTreeMap<crate::analysis::model::value::ValueId, ExportResolution> {
        let mut identities = BTreeMap::new();
        let Some(module) = self.module(importer) else {
            return identities;
        };
        let stream = module.local().facts().stream();
        for effect in module.local().effects().iter_effects() {
            for call in effect.calls() {
                let cref = stream.call_effect(call.event());
                let Some(provenance) = cref.provenance() else {
                    continue;
                };
                let Some(identity) = self.call_result_identity(importer, cref, provenance, session)
                else {
                    continue;
                };
                identities.insert(cref.result(), identity);
            }
        }
        identities
    }

    fn call_result_identity(
        &self,
        importer: ModuleId,
        call: CallEffectRef<'_>,
        provenance: &SymbolCallProvenance,
        session: &mut LinkingSession,
    ) -> Option<ExportResolution> {
        let target =
            self.qualified_function_target(importer, call.target(), provenance, session)?;
        self.target_return_identity(target, session)
    }

    fn target_return_identity(
        &self,
        target: crate::analysis::QualifiedFunctionId,
        session: &mut LinkingSession,
    ) -> Option<ExportResolution> {
        let target_effect = self.effect(target)?;
        if target_effect.is_invalid() {
            return None;
        }

        let mut resolution = None;
        let mut conflict = false;
        for returned in target_effect
            .returns()
            .iter()
            .filter(|returned| returned.parameter().is_none())
        {
            let candidate = match returned.provenance() {
                SymbolCallProvenance::ModuleExport { module, export } => {
                    self.resolve_imported_identity(target.module(), module, export, session)
                }
                SymbolCallProvenance::Global { name } => {
                    ExportResolution::Global { name: name.clone() }
                }
                SymbolCallProvenance::Local => self
                    .module_fact_stream(target.module())
                    .and_then(|stream| stream.values().static_string(returned.value()))
                    .map_or(ExportResolution::Unknown, |value| {
                        ExportResolution::StaticString {
                            value: value.to_owned(),
                        }
                    }),
                SymbolCallProvenance::Unknown(_) => ExportResolution::Unknown,
            };
            match resolution {
                None => resolution = Some(candidate),
                Some(ref previous) if previous != &candidate => conflict = true,
                _ => {}
            }
        }

        Some(match resolution {
            Some(identity) if !conflict => identity,
            _ => ExportResolution::Unknown,
        })
    }

    /// Build imported and namespace-member identities for one module.
    pub(super) fn module_identities(
        &self,
        module: ModuleId,
        session: &mut LinkingSession,
    ) -> ModuleIdentityMap {
        let mut identities = ModuleIdentityMap::new();
        let Some(project_module) = self.module(module) else {
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
                ModuleRequestRole::ReExport | ModuleRequestRole::StarExport => false,
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
        if visiting.len() >= MAX_EXPORT_DEPTH {
            identities.insert(
                ModuleExportKey::wildcard(prefix.clone()),
                ExportResolution::Unknown,
            );
            return;
        }
        if !visiting.insert(module) {
            identities.insert(
                ModuleExportKey::wildcard(prefix.clone()),
                ExportResolution::Unknown,
            );
            return;
        }

        // Collect star-exported entries first into a temp map with
        // star-vs-star conflict detection, so that conflicting star-derived
        // names are marked Ambiguous before direct exports are considered.
        let mut star_entries = ModuleIdentityMap::new();
        if let Some(project_module) = self.module(module) {
            for request_index in project_module.local().interface().star_exports() {
                let Some(request) = project_module.local().interface().request(*request_index)
                else {
                    continue;
                };
                let key = QualifiedRequestId::new(module, request.id());
                if let Some(LinkedModuleTarget::Internal { id }) = self.resolution_for(&key) {
                    let mut child_entries = ModuleIdentityMap::new();
                    self.collect_exported_identities(*id, prefix, visiting, &mut child_entries);
                    star_entries.merge_star_from(child_entries);
                }
            }
        }

        // Insert resolved export-table entries (authoritative direct/named
        // exports) after star exports so they always win.
        self.linked
            .exports
            .copy_identities_into(module, prefix, identities);

        // Merge star-exported entries, preserving direct exports.
        identities.merge_missing_from(star_entries);

        visiting.remove(&module);
    }

    /// Resolve a namespace request without guessing at unsupported targets.
    fn resolve_namespace(&self, module: ModuleId, request: &ModuleRequest) -> ExportResolution {
        let key = QualifiedRequestId::new(module, request.id());
        target_to_export_resolution(self.resolution_for(&key), request.specifier(), "*")
    }
}
