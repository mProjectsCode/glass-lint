use std::collections::BTreeSet;

use crate::api::{
    compiler::normalized::{
        NormalizedEvent, NormalizedLifecycle, NormalizedRoot, NormalizedSubject,
    },
    rule::query::IdentitySpec,
};

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
    pub(crate) local: bool,
    pub(crate) cross_call: bool,
    pub(crate) cross_file: bool,
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

/// Requirements computed during normalization for physical planning.
///
/// Each field contains the exact set of capabilities needed by the
/// normalized query.  Runtime preparation must consult these sets rather
/// than performing work unconditionally.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PlanRequirements {
    pub(crate) value_resolution: BTreeSet<ValueResolutionRequirement>,
    pub(crate) flow: FlowRequirements,
    pub(crate) project: BTreeSet<ProjectRequirement>,
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

    /// Whether any project-level identity work (module identities, overlays)
    /// is needed.
    pub(crate) fn needs_module_identities(&self) -> bool {
        self.project.iter().any(|requirement| {
            matches!(
                requirement,
                ProjectRequirement::ExactModuleExports
                    | ProjectRequirement::PackageModuleExports
                    | ProjectRequirement::ExactModuleNamespaces
                    | ProjectRequirement::PackageModuleNamespaces
            )
        })
    }

    /// Whether call-result identity resolution is needed.
    pub(crate) fn needs_call_result_identities(&self) -> bool {
        self.project
            .contains(&ProjectRequirement::CallResultIdentities)
            || self
                .value_resolution
                .contains(&ValueResolutionRequirement::CallResultIdentities)
    }

    /// Whether a project identity overlay is needed for any matched plan.
    pub(crate) fn needs_project_overlay(&self) -> bool {
        self.project.iter().any(|requirement| {
            matches!(
                requirement,
                ProjectRequirement::ExactModuleExports
                    | ProjectRequirement::PackageModuleExports
                    | ProjectRequirement::ExactModuleNamespaces
                    | ProjectRequirement::PackageModuleNamespaces
                    | ProjectRequirement::CallResultIdentities
            )
        })
    }

    pub(crate) fn merge_from(&mut self, other: &Self) {
        self.value_resolution
            .extend(other.value_resolution.iter().cloned());
        self.flow.local |= other.flow.local;
        self.flow.cross_call |= other.flow.cross_call;
        self.flow.cross_file |= other.flow.cross_file;
        self.project.extend(other.project.iter().cloned());
    }

    fn for_event(event: &NormalizedEvent) -> Self {
        Self {
            value_resolution: Self::value_resolution_for_event(event),
            flow: FlowRequirements::default(),
            project: Self::project_for_event(event),
        }
    }

    fn for_lifecycle(_lc: &NormalizedLifecycle) -> Self {
        Self {
            value_resolution: BTreeSet::new(),
            flow: FlowRequirements {
                local: true,
                cross_call: true,
                cross_file: false,
            },
            project: BTreeSet::new(),
        }
    }

    pub(crate) fn for_root(root: &NormalizedRoot) -> Self {
        match root {
            NormalizedRoot::Event(ev) => Self::for_event(ev),
            NormalizedRoot::Any(branches) => {
                let mut req = Self::default();
                for b in branches {
                    req.merge_from(&Self::for_root(b));
                }
                req
            }
            NormalizedRoot::Lifecycle(lc) => Self::for_lifecycle(lc),
        }
    }

    fn value_resolution_for_event(event: &NormalizedEvent) -> BTreeSet<ValueResolutionRequirement> {
        let mut set = BTreeSet::new();
        if !event.arguments.is_empty() {
            set.insert(ValueResolutionRequirement::LocalStaticValues);
        }
        if let Some(identity) = event_identity(event) {
            match identity {
                IdentitySpec::ModuleExport { .. } | IdentitySpec::PackageModuleExport { .. } => {
                    set.insert(ValueResolutionRequirement::ModuleIdentityValues);
                    set.insert(ValueResolutionRequirement::CallResultIdentities);
                }
                IdentitySpec::ModuleNamespace { .. }
                | IdentitySpec::PackageModuleNamespace { .. } => {
                    set.insert(ValueResolutionRequirement::ModuleIdentityValues);
                }
                _ => {}
            }
        }
        set
    }

    fn project_for_event(event: &NormalizedEvent) -> BTreeSet<ProjectRequirement> {
        let mut set = BTreeSet::new();
        if let Some(identity) = event_identity(event)
            && requires_project_overlay_spec(identity)
        {
            match identity {
                IdentitySpec::ModuleExport { .. } => {
                    set.insert(ProjectRequirement::ExactModuleExports);
                    set.insert(ProjectRequirement::CallResultIdentities);
                }
                IdentitySpec::PackageModuleExport { .. } => {
                    set.insert(ProjectRequirement::PackageModuleExports);
                    set.insert(ProjectRequirement::CallResultIdentities);
                }
                IdentitySpec::ModuleNamespace { .. } => {
                    set.insert(ProjectRequirement::ExactModuleNamespaces);
                }
                IdentitySpec::PackageModuleNamespace { .. } => {
                    set.insert(ProjectRequirement::PackageModuleNamespaces);
                }
                _ => {}
            }
        }
        set
    }
}

fn event_identity(event: &NormalizedEvent) -> Option<&IdentitySpec> {
    match &event.subject {
        NormalizedSubject::Direct { identity } => Some(identity),
        NormalizedSubject::Returned { producer, .. } => Some(producer),
        NormalizedSubject::Instance { constructor, .. } => Some(constructor),
    }
}

fn requires_project_overlay_spec(identity: &IdentitySpec) -> bool {
    matches!(
        identity,
        IdentitySpec::ModuleExport { .. }
            | IdentitySpec::PackageModuleExport { .. }
            | IdentitySpec::ModuleNamespace { .. }
            | IdentitySpec::PackageModuleNamespace { .. }
    )
}
