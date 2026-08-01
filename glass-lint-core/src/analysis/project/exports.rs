//! Post-link export identity lookup.

use smol_str::SmolStr;

use super::{resolver::ExportResolver, state::LinkingSession};
use crate::analysis::{ExportResolution, ModuleId, ProjectSemanticModel};

impl ProjectSemanticModel {
    /// Resolve an authored module/export pair across all matching requests.
    /// Conflicting request answers are rejected as ambiguous.
    pub(in crate::analysis) fn resolve_imported_identity(
        &self,
        importer: ModuleId,
        authored_module: &SmolStr,
        authored_export: &SmolStr,
        session: &mut LinkingSession,
    ) -> ExportResolution {
        ExportResolver::new(
            &self.modules,
            &self.resolutions,
            &self.exports,
            &mut session.lookup_cache,
        )
        .resolve_imported_identity(importer, authored_module, authored_export)
    }
}
