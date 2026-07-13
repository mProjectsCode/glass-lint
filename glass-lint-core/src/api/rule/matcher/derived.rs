use super::CallProvenance;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstructorMatcher {
    pub(crate) name: String,
    pub(crate) provenance: CallProvenance,
}

impl ConstructorMatcher {
    pub fn heuristic(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provenance: CallProvenance::Any,
        }
    }

    pub fn global(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provenance: CallProvenance::Global,
        }
    }

    pub fn module_export(module: impl Into<String>, export: impl Into<String>) -> Self {
        Self {
            name: export.into(),
            provenance: CallProvenance::ModuleExport {
                module: module.into(),
            },
        }
    }

    #[must_use]
    pub fn evidence_symbol(&self) -> String {
        match &self.provenance {
            CallProvenance::Any | CallProvenance::Global => self.name.clone(),
            CallProvenance::ModuleExport { module } => format!("{module}.{}", self.name),
        }
    }

    pub(crate) fn sort_key(&self) -> (&str, &str) {
        match &self.provenance {
            CallProvenance::Any => ("any", &self.name),
            CallProvenance::Global => ("global", &self.name),
            CallProvenance::ModuleExport { module } => (module, &self.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassMatcher {
    pub(crate) name: String,
    pub(crate) provenance: CallProvenance,
}

impl ClassMatcher {
    pub fn heuristic(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provenance: CallProvenance::Any,
        }
    }

    pub fn module_export(module: impl Into<String>, export: impl Into<String>) -> Self {
        Self {
            name: export.into(),
            provenance: CallProvenance::ModuleExport {
                module: module.into(),
            },
        }
    }

    #[must_use]
    pub fn evidence_symbol(&self) -> String {
        match &self.provenance {
            CallProvenance::Any | CallProvenance::Global => self.name.clone(),
            CallProvenance::ModuleExport { module } => format!("{module}.{}", self.name),
        }
    }

    pub(crate) fn sort_key(&self) -> (&str, &str) {
        match &self.provenance {
            CallProvenance::Any => ("any", &self.name),
            CallProvenance::Global => ("global", &self.name),
            CallProvenance::ModuleExport { module } => (module, &self.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnedMemberCallMatcher {
    pub(crate) source: String,
    pub(crate) member: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnedMemberReadMatcher {
    pub(crate) source: String,
    pub(crate) member: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceMemberCallMatcher {
    pub(crate) module: String,
    pub(crate) export: String,
    pub(crate) member: String,
}
