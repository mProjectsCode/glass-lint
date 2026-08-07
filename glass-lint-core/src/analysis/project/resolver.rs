//! Shared bounded export lookup for the linker and post-link semantic model.

use std::collections::BTreeSet;

use smol_str::{SmolStr, ToSmolStr};

use crate::{
    analysis::{
        ExportResolution, LinkedModuleTarget, ModuleId, ProjectModule, QualifiedRequestId,
        model::module::{DEFAULT_EXPORT, ModuleRequestId, ModuleRequestRole},
        project::{
            model::MAX_EXPORT_DEPTH,
            state::{ExportLookupCache, ExportLookupCacheResult, ExportTable, QualifiedExportId},
        },
    },
    project::is_internal_module_request as is_internal_request,
};

/// Shared direct/star export resolver used by both linking phases.
pub(super) trait ProjectLookup {
    fn module(&self, module: ModuleId) -> Option<&ProjectModule>;

    fn request_target(
        &self,
        module: ModuleId,
        request: ModuleRequestId,
    ) -> Option<&LinkedModuleTarget>;
}

/// Borrowed lookup view shared by transient linking and the final model.
pub(super) struct ProjectLookupView<'a> {
    modules: &'a std::collections::BTreeMap<ModuleId, ProjectModule>,
    resolutions: &'a std::collections::BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
}

impl<'a> ProjectLookupView<'a> {
    pub(super) fn new(
        modules: &'a std::collections::BTreeMap<ModuleId, ProjectModule>,
        resolutions: &'a std::collections::BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
    ) -> Self {
        Self {
            modules,
            resolutions,
        }
    }
}

impl ProjectLookup for ProjectLookupView<'_> {
    fn module(&self, module: ModuleId) -> Option<&ProjectModule> {
        self.modules.get(&module)
    }

    fn request_target(
        &self,
        module: ModuleId,
        request: ModuleRequestId,
    ) -> Option<&LinkedModuleTarget> {
        self.modules.get(&module)?;
        self.resolutions
            .get(&QualifiedRequestId::new(module, request))
    }
}

pub(super) struct ExportResolver<'a> {
    project: &'a dyn ProjectLookup,
    exports: &'a ExportTable,
    cache: &'a mut ExportLookupCache,
}

impl<'a> ExportResolver<'a> {
    pub(super) fn new(
        project: &'a dyn ProjectLookup,
        exports: &'a ExportTable,
        cache: &'a mut ExportLookupCache,
    ) -> Self {
        Self {
            project,
            exports,
            cache,
        }
    }

    /// Resolve an authored module/export pair across all matching requests.
    /// Conflicting request answers remain unknown rather than source-order
    /// dependent.
    pub(super) fn resolve_imported_identity(
        &mut self,
        importer: ModuleId,
        authored_module: &SmolStr,
        authored_export: &SmolStr,
    ) -> ExportResolution {
        let Some(interface) = self
            .project
            .module(importer)
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
            let candidate = match self.project.request_target(importer, request.id()) {
                Some(LinkedModuleTarget::Internal { id }) => self
                    .lookup_export(
                        &QualifiedExportId::new(*id, authored_export.clone()),
                        &mut BTreeSet::new(),
                    )
                    .unwrap_or(ExportResolution::Unknown),
                target => target_to_export_resolution(target, authored_module, authored_export),
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
    pub(super) fn lookup_export(
        &mut self,
        id: &QualifiedExportId,
        visiting: &mut BTreeSet<QualifiedExportId>,
    ) -> Option<ExportResolution> {
        if let Some(resolved) = self.exports.resolve(id) {
            return Some(resolved.clone());
        }
        if let ExportLookupCacheResult::Hit(cached) = self.cache.lookup(id) {
            return cached.cloned();
        }
        if visiting.len() >= MAX_EXPORT_DEPTH || !visiting.insert(id.clone()) {
            return None;
        }
        if id.name() == DEFAULT_EXPORT {
            visiting.remove(id);
            return None;
        }
        let is_unknown = self
            .project
            .module(id.module())
            .map(|module| module.local().interface().is_unknown())?;
        if is_unknown {
            return Some(ExportResolution::Unknown);
        }
        let (candidate, saw_unknown) = self.walk_star_exports(id, visiting);
        visiting.remove(id);

        if let Some(resolved) = self.exports.resolve(id) {
            return Some(resolved.clone());
        }
        let result = if saw_unknown { None } else { candidate };
        self.cache.insert(id.clone(), result.clone());
        result
    }

    fn walk_star_exports(
        &mut self,
        id: &QualifiedExportId,
        visiting: &mut BTreeSet<QualifiedExportId>,
    ) -> (Option<ExportResolution>, bool) {
        let module = id.module();
        let export_name = id.name().clone();
        let star_exports = self
            .project
            .module(module)
            .map(|module| {
                module
                    .local()
                    .interface()
                    .star_exports()
                    .copied()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut candidate = None;
        let mut saw_unknown = false;
        for request_index in star_exports {
            let Some(request) = self
                .project
                .module(module)
                .and_then(|module| module.local().interface().request(request_index))
            else {
                saw_unknown = true;
                continue;
            };
            let candidate_export = match self.project.request_target(module, request.id()) {
                Some(LinkedModuleTarget::Internal { id: target }) => self.lookup_export(
                    &QualifiedExportId::new(*target, export_name.clone()),
                    visiting,
                ),
                Some(target) => Some(linked_target_to_export_resolution(target, &export_name)),
                None => None,
            };
            match candidate_export {
                Some(resolved)
                    if candidate
                        .as_ref()
                        .is_none_or(|existing| existing == &resolved) =>
                {
                    candidate = Some(resolved);
                }
                Some(_) => return (Some(ExportResolution::Ambiguous), false),
                None => saw_unknown = true,
            }
        }
        (candidate, saw_unknown)
    }
}

/// Convert a linked request target into an export identity.
pub(super) fn target_to_export_resolution(
    target: Option<&LinkedModuleTarget>,
    specifier: &SmolStr,
    export: &str,
) -> ExportResolution {
    match target {
        None if is_internal_request(specifier) => ExportResolution::Unknown,
        None => ExportResolution::External {
            module: specifier.clone(),
            export: export.into(),
        },
        Some(target) => linked_target_to_export_resolution(target, export),
    }
}

/// Convert a known linked target without applying the authored-specifier
/// fallback used when a target is absent.
pub(super) fn linked_target_to_export_resolution(
    target: &LinkedModuleTarget,
    export: &str,
) -> ExportResolution {
    match target {
        LinkedModuleTarget::External { package } => ExportResolution::External {
            module: package.to_smolstr(),
            export: export.into(),
        },
        LinkedModuleTarget::Builtin { name } => ExportResolution::External {
            module: name.to_smolstr(),
            export: export.into(),
        },
        LinkedModuleTarget::Internal { id } => ExportResolution::Qualified {
            module: *id,
            export: export.into(),
        },
        LinkedModuleTarget::Missing
        | LinkedModuleTarget::OutsideProject { .. }
        | LinkedModuleTarget::Unsupported { .. } => ExportResolution::Unknown,
    }
}
