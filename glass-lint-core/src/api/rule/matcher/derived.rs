//! Constructor, class, and returned/instance member matcher declarations.

use super::CallProvenance;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Matcher for a constructor invocation.
pub struct ConstructorMatcher {
    /// Constructor name/export.
    pub name: String,
    /// Required provenance mode.
    pub provenance: CallProvenance,
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

    pub fn sort_key(&self) -> (&str, &str) {
        match &self.provenance {
            CallProvenance::Any => ("any", &self.name),
            CallProvenance::Global => ("global", &self.name),
            CallProvenance::ModuleExport { module } => (module, &self.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Matcher for a class declaration or class identity.
pub struct ClassMatcher {
    /// Class name/export.
    pub name: String,
    /// Required provenance mode.
    pub provenance: CallProvenance,
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

    pub fn sort_key(&self) -> (&str, &str) {
        match &self.provenance {
            CallProvenance::Any => ("any", &self.name),
            CallProvenance::Global => ("global", &self.name),
            CallProvenance::ModuleExport { module } => (module, &self.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Member call on an object returned from a rooted source.
pub struct ReturnedMemberCallMatcher {
    /// Rooted source chain.
    pub source: String,
    /// Member method name.
    pub member: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Member read on an object returned from a rooted source.
pub struct ReturnedMemberReadMatcher {
    /// Rooted source chain.
    pub source: String,
    /// Member name.
    pub member: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Member call on a proven module-exported instance.
pub struct InstanceMemberCallMatcher {
    /// Exporting module specifier.
    pub module: String,
    /// Exported constructor/factory name.
    pub export: String,
    /// Instance member name.
    pub(crate) member: String,
}
