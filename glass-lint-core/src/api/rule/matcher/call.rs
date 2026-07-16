//! Callable matcher declarations and provenance modes.

use super::{ArgumentConstraint, ArgumentMatcher, ValueMatcher};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Matcher for a callable symbol and optional argument predicates.
pub struct CallMatcher {
    /// Callable name or rooted symbol spelling.
    pub name: String,
    /// Required call provenance mode.
    pub provenance: CallProvenance,
    /// Predicates attached to zero-based argument positions.
    pub arguments: Vec<ArgumentConstraint>,
}

impl CallMatcher {
    /// Construct a spelling-based heuristic matcher.
    pub fn heuristic(name: impl Into<String>) -> Self {
        Self::new(name, CallProvenance::Any)
    }

    /// Construct a matcher requiring an unshadowed configured global.
    pub fn global(name: impl Into<String>) -> Self {
        Self::new(name, CallProvenance::Global)
    }

    /// Construct a matcher for an export from a named module.
    pub fn module_export(module: impl Into<String>, export: impl Into<String>) -> Self {
        Self::new(
            export,
            CallProvenance::ModuleExport {
                module: module.into(),
            },
        )
    }

    fn new(name: impl Into<String>, provenance: CallProvenance) -> Self {
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
    pub fn static_string_arg(self, index: usize) -> Self {
        self.arg(index, ValueMatcher::static_string())
    }

    #[must_use]
    /// Restrict a static string argument to exact allowed values.
    pub fn arg_string<I, S>(self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arg(index, ValueMatcher::static_string().equals_any(values))
    }

    #[must_use]
    /// Attach an arbitrary value matcher to one argument.
    pub fn arg_value(self, index: usize, value: impl Into<ValueMatcher>) -> Self {
        self.arg(index, value.into())
    }

    /// Return the display/evidence symbol for this matcher.
    pub fn evidence_symbol(&self) -> String {
        match &self.provenance {
            CallProvenance::Any | CallProvenance::Global => self.name.clone(),
            CallProvenance::ModuleExport { module } => format!("{module}.{}", self.name),
        }
    }

    /// Return the deterministic normalization sort key.
    pub fn sort_key(&self) -> (&str, &str) {
        match &self.provenance {
            CallProvenance::Any => ("any", &self.name),
            CallProvenance::Global => ("global", &self.name),
            CallProvenance::ModuleExport { module } => (module, &self.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Provenance requirement for a callable matcher.
pub enum CallProvenance {
    /// Accept any callable spelling/provenance.
    Any,
    /// Require a configured unshadowed global.
    Global,
    /// Require an export from the configured module.
    ModuleExport { module: String },
}
