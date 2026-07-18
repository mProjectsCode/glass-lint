//! Constructor, class, and returned/instance member matcher declarations.

use super::SymbolProvenance;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Matcher for a constructor invocation.
pub struct ConstructorMatcher {
    /// Constructor name/export.
    pub name: String,
    /// Required provenance mode.
    pub provenance: SymbolProvenance,
}

impl ConstructorMatcher {
    pub fn heuristic(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provenance: SymbolProvenance::Any,
        }
    }

    pub fn global(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provenance: SymbolProvenance::Global,
        }
    }

    pub fn module_export(module: impl Into<String>, export: impl Into<String>) -> Self {
        Self {
            name: export.into(),
            provenance: SymbolProvenance::ModuleExport {
                module: module.into(),
            },
        }
    }

    pub fn package_export(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, String> {
        Ok(Self {
            name: export.into(),
            provenance: SymbolProvenance::PackageModuleExport {
                module: super::super::ModuleSpecifierPattern::package(module)?,
            },
        })
    }

    #[must_use]
    pub fn evidence_symbol(&self) -> String {
        match &self.provenance {
            SymbolProvenance::Any | SymbolProvenance::Global => self.name.clone(),
            SymbolProvenance::ModuleExport { module } => format!("{module}.{}", self.name),
            SymbolProvenance::PackageModuleExport { module } => format!("{module}.{}", self.name),
        }
    }

    pub fn sort_key(&self) -> (&str, &str) {
        match &self.provenance {
            SymbolProvenance::Any => ("any", &self.name),
            SymbolProvenance::Global => ("global", &self.name),
            SymbolProvenance::ModuleExport { module } => (module, &self.name),
            SymbolProvenance::PackageModuleExport { module } => (module.as_str(), &self.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Matcher for a class declaration or class identity.
pub struct ClassMatcher {
    /// Class name/export.
    pub name: String,
    /// Required provenance mode.
    pub provenance: SymbolProvenance,
}

impl ClassMatcher {
    pub fn heuristic(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provenance: SymbolProvenance::Any,
        }
    }

    pub fn module_export(module: impl Into<String>, export: impl Into<String>) -> Self {
        Self {
            name: export.into(),
            provenance: SymbolProvenance::ModuleExport {
                module: module.into(),
            },
        }
    }

    pub fn package_export(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, String> {
        Ok(Self {
            name: export.into(),
            provenance: SymbolProvenance::PackageModuleExport {
                module: super::super::ModuleSpecifierPattern::package(module)?,
            },
        })
    }

    #[must_use]
    pub fn evidence_symbol(&self) -> String {
        match &self.provenance {
            SymbolProvenance::Any | SymbolProvenance::Global => self.name.clone(),
            SymbolProvenance::ModuleExport { module } => format!("{module}.{}", self.name),
            SymbolProvenance::PackageModuleExport { module } => format!("{module}.{}", self.name),
        }
    }

    pub fn sort_key(&self) -> (&str, &str) {
        match &self.provenance {
            SymbolProvenance::Any => ("any", &self.name),
            SymbolProvenance::Global => ("global", &self.name),
            SymbolProvenance::ModuleExport { module } => (module, &self.name),
            SymbolProvenance::PackageModuleExport { module } => (module.as_str(), &self.name),
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
    /// Optional boundary-aware package pattern for the exporting module.
    pub module_pattern: Option<super::super::ModuleSpecifierPattern>,
    /// Exported constructor/factory name.
    pub export: String,
    /// Instance member name.
    pub(crate) member: String,
}
