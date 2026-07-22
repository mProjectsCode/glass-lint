//! Constructor, class, and returned/instance member matcher declarations.

use crate::api::rule::{MatcherBuildError, ModuleSpecifierPattern, matcher::SymbolProvenance};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Matcher for a constructor invocation.
pub struct ConstructorMatcher {
    /// Constructor name/export.
    name: String,
    /// Required provenance mode.
    provenance: SymbolProvenance,
}

impl ConstructorMatcher {
    /// Borrow the constructor name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Borrow the provenance mode.
    pub fn provenance(&self) -> &SymbolProvenance {
        &self.provenance
    }

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
    ) -> Result<Self, MatcherBuildError> {
        Ok(Self {
            name: export.into(),
            provenance: SymbolProvenance::PackageModuleExport {
                module: ModuleSpecifierPattern::package(module)?,
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

    fn normalize(&mut self) {
        self.name = self.name.trim().to_string();
        self.provenance.normalize();
    }

    pub fn normalize_all(values: &mut Vec<Self>) {
        for value in values.iter_mut() {
            value.normalize();
        }
        values.retain(|value| !value.name().is_empty());
        values.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        values.dedup();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Matcher for a class declaration or class identity.
pub struct ClassMatcher {
    /// Class name/export.
    name: String,
    /// Required provenance mode.
    provenance: SymbolProvenance,
}

impl ClassMatcher {
    /// Borrow the class name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Borrow the provenance mode.
    pub fn provenance(&self) -> &SymbolProvenance {
        &self.provenance
    }

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
    ) -> Result<Self, MatcherBuildError> {
        Ok(Self {
            name: export.into(),
            provenance: SymbolProvenance::PackageModuleExport {
                module: ModuleSpecifierPattern::package(module)?,
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

    fn normalize(&mut self) {
        self.name = self.name.trim().to_string();
        self.provenance.normalize();
    }

    pub fn normalize_all(values: &mut Vec<Self>) {
        for value in values.iter_mut() {
            value.normalize();
        }
        values.retain(|value| !value.name().is_empty());
        values.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        values.dedup();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Member call on an object returned from a rooted source.
pub struct ReturnedMemberCallMatcher {
    /// Rooted source chain.
    source: String,
    /// Member method name.
    member: String,
}

impl ReturnedMemberCallMatcher {
    pub fn new(source: impl Into<String>, member: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            member: member.into(),
        }
    }

    /// Borrow the rooted source chain.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Borrow the member name.
    pub fn member(&self) -> &str {
        &self.member
    }

    fn normalize(&mut self) {
        self.source = crate::api::rule::matcher::canonical_rooted_chain(
            &crate::api::rule::matcher::normalize_member_chain(&self.source),
        )
        .to_string();
        self.member = self.member.trim().to_string();
    }

    pub(crate) fn normalize_all(values: &mut Vec<Self>) {
        for value in values.iter_mut() {
            value.normalize();
        }
        values.retain(|value| !value.source().is_empty() && !value.member().is_empty());
        values.sort_by(|left, right| {
            (&left.source, &left.member).cmp(&(&right.source, &right.member))
        });
        values.dedup();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Member read on an object returned from a rooted source.
pub struct ReturnedMemberReadMatcher {
    /// Rooted source chain.
    source: String,
    /// Member name.
    member: String,
}

impl ReturnedMemberReadMatcher {
    pub fn new(source: impl Into<String>, member: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            member: member.into(),
        }
    }

    /// Borrow the rooted source chain.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Borrow the member name.
    pub fn member(&self) -> &str {
        &self.member
    }

    fn normalize(&mut self) {
        self.source = crate::api::rule::matcher::canonical_rooted_chain(
            &crate::api::rule::matcher::normalize_member_chain(&self.source),
        )
        .to_string();
        self.member = self.member.trim().to_string();
    }

    pub(crate) fn normalize_all(values: &mut Vec<Self>) {
        for value in values.iter_mut() {
            value.normalize();
        }
        values.retain(|value| !value.source().is_empty() && !value.member().is_empty());
        values.sort_by(|left, right| {
            (&left.source, &left.member).cmp(&(&right.source, &right.member))
        });
        values.dedup();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Member call on a proven module-exported instance.
pub struct InstanceMemberCallMatcher {
    /// Exporting module specifier.
    module: String,
    /// Optional boundary-aware package pattern for the exporting module.
    module_pattern: Option<ModuleSpecifierPattern>,
    /// Exported constructor/factory name.
    export: String,
    /// Instance member name.
    member: String,
}

impl InstanceMemberCallMatcher {
    pub fn new(
        module: impl Into<String>,
        export: impl Into<String>,
        member: impl Into<String>,
    ) -> Self {
        Self {
            module: module.into(),
            module_pattern: None,
            export: export.into(),
            member: member.into(),
        }
    }

    pub(crate) fn with_package(
        module: String,
        pattern: ModuleSpecifierPattern,
        export: impl Into<String>,
        member: impl Into<String>,
    ) -> Self {
        Self {
            module,
            module_pattern: Some(pattern),
            export: export.into(),
            member: member.into(),
        }
    }

    /// Borrow the exporting module specifier.
    pub fn module(&self) -> &str {
        &self.module
    }

    /// Borrow the optional package pattern.
    pub fn module_pattern(&self) -> Option<&ModuleSpecifierPattern> {
        self.module_pattern.as_ref()
    }

    /// Borrow the exported constructor/factory name.
    pub fn export(&self) -> &str {
        &self.export
    }

    /// Borrow the instance member name.
    pub fn member(&self) -> &str {
        &self.member
    }

    fn normalize(&mut self) {
        self.module = self.module.trim().to_string();
        self.export = self.export.trim().to_string();
        self.member = self.member.trim().to_string();
    }

    pub(crate) fn normalize_all(values: &mut Vec<Self>) {
        for value in values.iter_mut() {
            value.normalize();
        }
        values.retain(|value| {
            !value.module().is_empty() && !value.export().is_empty() && !value.member().is_empty()
        });
        values.sort_by(|left, right| {
            (&left.module, &left.export, &left.member).cmp(&(
                &right.module,
                &right.export,
                &right.member,
            ))
        });
        values.dedup();
    }
}
