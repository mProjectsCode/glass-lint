//! Post-link export identity lookup.

use std::collections::BTreeMap;

use smol_str::SmolStr;

use super::{
    model::QualifiedRequestId,
    resolver::ExportResolver,
    state::{ExportTable, LinkingSession},
};
use crate::analysis::{ExportResolution, LinkedModuleTarget, ModuleId, ProjectModule};

/// Resolve an authored module/export pair across all matching requests.
/// Conflicting request answers are rejected as ambiguous.
pub(super) fn resolve_imported_identity(
    modules: &BTreeMap<ModuleId, ProjectModule>,
    resolutions: &BTreeMap<QualifiedRequestId, LinkedModuleTarget>,
    exports: &ExportTable,
    importer: ModuleId,
    authored_module: &SmolStr,
    authored_export: &SmolStr,
    session: &mut LinkingSession,
) -> ExportResolution {
    ExportResolver::new(modules, resolutions, exports, &mut session.lookup_cache)
        .resolve_imported_identity(importer, authored_module, authored_export)
}
