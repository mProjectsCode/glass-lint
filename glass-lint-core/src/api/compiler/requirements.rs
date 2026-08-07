use std::collections::BTreeSet;

use crate::api::compiler::IdentityConstraint;

// ── Plan requirements ─────────────────────────────────────────────────────

/// Which value-resolution capabilities the physical plan needs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ValueResolutionRequirement {
    LocalStaticValues,
    ModuleIdentityValues,
    CallResultIdentities,
}

/// Whether local, cross-call, or cross-file flow projection is required.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct FlowRequirements {
    local: bool,
    cross_call: bool,
    cross_file: bool,
}

impl FlowRequirements {
    pub(crate) fn new(local: bool, cross_call: bool, cross_file: bool) -> Self {
        Self {
            local,
            cross_call,
            cross_file,
        }
    }

    pub(crate) fn local(&self) -> bool {
        self.local
    }

    pub(crate) fn cross_call(&self) -> bool {
        self.cross_call
    }

    pub(crate) fn cross_file(&self) -> bool {
        self.cross_file
    }
}

/// Which project-level preparation the physical plan needs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ProjectRequirement {
    ExactModuleExports,
    PackageModuleExports,
    ExactModuleNamespaces,
    PackageModuleNamespaces,
    CallResultIdentities,
}

impl ProjectRequirement {
    fn needs_module_identities(&self) -> bool {
        matches!(
            self,
            Self::ExactModuleExports
                | Self::PackageModuleExports
                | Self::ExactModuleNamespaces
                | Self::PackageModuleNamespaces
        )
    }

    fn needs_call_result_identities(&self) -> bool {
        matches!(self, Self::CallResultIdentities)
    }

    fn needs_project_overlay(&self) -> bool {
        self.needs_module_identities() || self.needs_call_result_identities()
    }
}

/// Requirements computed during normalization for physical planning.
///
/// Each capability carries the exact set of work needed by the normalized
/// query. Runtime preparation must consult these sets rather than performing
/// work unconditionally. Capabilities are added only through the `require_*`
/// mutation methods, which centralize the legal capability transitions; the
/// collections and flow flags stay private.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PlanRequirements {
    value_resolution: BTreeSet<ValueResolutionRequirement>,
    flow: FlowRequirements,
    project: BTreeSet<ProjectRequirement>,
}

impl PlanRequirements {
    #[cfg(test)]
    pub(crate) fn value_resolution(&self) -> &BTreeSet<ValueResolutionRequirement> {
        &self.value_resolution
    }

    pub(crate) fn flow(&self) -> &FlowRequirements {
        &self.flow
    }

    #[cfg(test)]
    pub(crate) fn project_requirements(&self) -> &BTreeSet<ProjectRequirement> {
        &self.project
    }

    /// Record that matching must resolve the static values of call arguments.
    pub(crate) fn require_local_static_values(&mut self) {
        self.value_resolution
            .insert(ValueResolutionRequirement::LocalStaticValues);
    }

    /// Record the value-resolution and project capabilities required to match
    /// the given identity constraint.
    pub(crate) fn require_identity(&mut self, identity: &IdentityConstraint) {
        match identity {
            IdentityConstraint::ModuleExport { .. } => {
                self.value_resolution.extend([
                    ValueResolutionRequirement::ModuleIdentityValues,
                    ValueResolutionRequirement::CallResultIdentities,
                ]);
                self.project.extend([
                    ProjectRequirement::ExactModuleExports,
                    ProjectRequirement::CallResultIdentities,
                ]);
            }
            IdentityConstraint::PackageModuleExport { .. } => {
                self.value_resolution.extend([
                    ValueResolutionRequirement::ModuleIdentityValues,
                    ValueResolutionRequirement::CallResultIdentities,
                ]);
                self.project.extend([
                    ProjectRequirement::PackageModuleExports,
                    ProjectRequirement::CallResultIdentities,
                ]);
            }
            IdentityConstraint::ModuleNamespace { .. } => {
                self.value_resolution
                    .insert(ValueResolutionRequirement::ModuleIdentityValues);
                self.project
                    .insert(ProjectRequirement::ExactModuleNamespaces);
            }
            IdentityConstraint::PackageModuleNamespace { .. } => {
                self.value_resolution
                    .insert(ValueResolutionRequirement::ModuleIdentityValues);
                self.project
                    .insert(ProjectRequirement::PackageModuleNamespaces);
            }
            _ => {}
        }
    }

    /// Record that matching needs local flow projection.
    pub(crate) fn require_local_flow(&mut self) {
        self.flow.local = true;
    }

    /// Record that matching needs cross-call flow projection.
    pub(crate) fn require_cross_call_flow(&mut self) {
        self.flow.cross_call = true;
    }

    /// Whether any project-level identity work (module identities, overlays)
    /// is needed.
    pub(crate) fn needs_module_identities(&self) -> bool {
        self.project
            .iter()
            .any(ProjectRequirement::needs_module_identities)
    }

    /// Whether call-result identity resolution is needed.
    pub(crate) fn needs_call_result_identities(&self) -> bool {
        self.project
            .iter()
            .any(ProjectRequirement::needs_call_result_identities)
            || self
                .value_resolution
                .contains(&ValueResolutionRequirement::CallResultIdentities)
    }

    /// Whether a project identity overlay is needed for any matched plan.
    pub(crate) fn needs_project_overlay(&self) -> bool {
        self.project
            .iter()
            .any(ProjectRequirement::needs_project_overlay)
    }

    pub(crate) fn merge_from(&mut self, other: &Self) {
        self.value_resolution
            .extend(other.value_resolution.iter().cloned());
        self.flow.local |= other.flow.local;
        self.flow.cross_call |= other.flow.cross_call;
        self.flow.cross_file |= other.flow.cross_file;
        self.project.extend(other.project.iter().cloned());
    }
}
