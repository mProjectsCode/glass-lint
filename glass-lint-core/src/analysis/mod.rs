//! Private semantic analysis and project linking.
//!
//! Local construction and matcher projection are deliberately separate. A
//! source is parsed and semantically visited once into a matcher-independent
//! model; rules query a linked project model afterwards.
//!
//! Local scopes and value arenas remain partitioned by module. Linking adds
//! qualified identities and bounded flow overlays, never lexical facts from
//! one module into another.

use crate::project::{LinkedModuleTarget, ModuleId};

mod facts;
pub mod flow;
mod local;
mod matching;
pub mod model;
mod module_request;
pub mod project;
mod resolution;
mod scope;
mod semantic;
mod syntax;
pub mod trace;

pub use local::{
    ArtifactCacheHandle, ArtifactCacheKey, LocalArtifact, LocatedSourceContext, ProjectModule,
    SemanticArtifact,
};
pub use matching::display_span;
pub(in crate::analysis) use project::model::{ExportResolution, QualifiedFunctionId};
pub use project::model::{ProjectSemanticModel, QualifiedRequestId, ResolvedLinkInput};
pub(in crate::analysis) use semantic::budget::SemanticBudget;
pub use semantic::{
    SemanticAnalyzer,
    status::{AnalysisStatus, IncompleteReason, StatusScope},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis) enum DerivedPhaseAvailability {
    #[default]
    Enabled,
    DisabledByIncompleteAnalysis,
}

impl DerivedPhaseAvailability {
    pub(in crate::analysis) const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// Derived-phase availability is all-or-nothing: incomplete analysis disables
/// every derived phase together. Reintroduce per-phase granularity only if a
/// genuinely independent per-phase disable is added later.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::analysis) struct DerivedPhaseCapabilities {
    availability: DerivedPhaseAvailability,
}

impl DerivedPhaseCapabilities {
    pub(in crate::analysis) const fn enabled() -> Self {
        Self {
            availability: DerivedPhaseAvailability::Enabled,
        }
    }

    pub(in crate::analysis) fn disable_derived_phases(&mut self) {
        self.availability = DerivedPhaseAvailability::DisabledByIncompleteAnalysis;
    }

    pub(in crate::analysis) const fn availability(self) -> DerivedPhaseAvailability {
        self.availability
    }
}

#[cfg(test)]
mod tests;
