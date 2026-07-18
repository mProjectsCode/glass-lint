//! Callable matcher declarations and provenance modes.

use super::{ArgumentConstraint, ArgumentMatcher, ValueMatcher};
use crate::api::rule::ModuleSpecifierPattern;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Matcher for a callable symbol and optional argument predicates.
pub struct CallMatcher {
    /// Callable name or rooted symbol spelling.
    pub name: String,
    /// Required call provenance mode.
    pub provenance: SymbolProvenance,
    /// Predicates attached to zero-based argument positions.
    pub arguments: Vec<ArgumentConstraint>,
}

impl CallMatcher {
    /// Construct a spelling-based heuristic matcher.
    pub fn heuristic(name: impl Into<String>) -> Self {
        Self::new(name, SymbolProvenance::Any)
    }

    /// Construct a matcher requiring an unshadowed configured global.
    pub fn global(name: impl Into<String>) -> Self {
        Self::new(name, SymbolProvenance::Global)
    }

    /// Construct a matcher for an export from a named module.
    pub fn module_export(module: impl Into<String>, export: impl Into<String>) -> Self {
        Self::new(
            export,
            SymbolProvenance::ModuleExport {
                module: module.into(),
            },
        )
    }

    pub fn package_export(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, String> {
        Ok(Self::new(
            export,
            SymbolProvenance::PackageModuleExport {
                module: ModuleSpecifierPattern::package(module)?,
            },
        ))
    }

    fn new(name: impl Into<String>, provenance: SymbolProvenance) -> Self {
        Self {
            name: name.into(),
            provenance,
            arguments: Vec::new(),
        }
    }

    #[must_use]
    /// Add a predicate for one argument position.
    pub fn arg(mut self, index: usize, matcher: impl Into<ArgumentMatcher>) -> Self {
        self.arguments.push(ArgumentConstraint {
            index,
            matcher: matcher.into(),
        });
        self
    }

    #[must_use]
    /// Require a proven static string at one argument position.
    pub fn arg_static_string(self, index: usize) -> Self {
        self.arg(index, ValueMatcher::static_string())
    }

    #[must_use]
    /// Restrict a static string argument to exact allowed values.
    pub fn arg_static_strings<I, S>(self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arg(index, ValueMatcher::static_string().equals_any(values))
    }

    #[must_use]
    pub fn arg_static_string_contains<I, S>(self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arg(index, ValueMatcher::static_string().contains_any(values))
    }

    #[must_use]
    pub fn arg_object_property_value(
        self,
        index: usize,
        property: impl Into<String>,
        value: ValueMatcher,
    ) -> Self {
        self.arg(
            index,
            ArgumentMatcher::object_property_value(property, value),
        )
    }

    #[must_use]
    pub fn arg_object_keys<I, S>(self, index: usize, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arg(index, ArgumentMatcher::object_keys(keys))
    }

    /// Return the display/evidence symbol for this matcher.
    pub fn evidence_symbol(&self) -> String {
        match &self.provenance {
            SymbolProvenance::Any | SymbolProvenance::Global => self.name.clone(),
            SymbolProvenance::ModuleExport { module } => format!("{module}.{}", self.name),
            SymbolProvenance::PackageModuleExport { module } => format!("{module}.{}", self.name),
        }
    }

    /// Return the deterministic normalization sort key.
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
/// Provenance requirement for a callable matcher.
pub enum SymbolProvenance {
    /// Accept any callable spelling/provenance.
    Any,
    /// Require a configured unshadowed global.
    Global,
    /// Require an export from the configured module.
    ModuleExport {
        module: String,
    },
    PackageModuleExport {
        module: ModuleSpecifierPattern,
    },
}
