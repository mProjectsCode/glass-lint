//! Validated rule taxonomy and report severity types.

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(transparent)]
/// Provider-defined hierarchical category name.
pub struct Category(String);

impl Category {
    /// Trim and store a candidate category name.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into().trim().to_string())
    }

    #[must_use]
    /// Borrow the canonical category spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the category obeys the lowercase path grammar.
    pub fn is_valid(&self) -> bool {
        !self.0.is_empty()
            && self.0.chars().enumerate().all(|(index, character)| {
                (index == 0 && character.is_ascii_lowercase())
                    || (index > 0
                        && (character.is_ascii_lowercase()
                            || character.is_ascii_digit()
                            || character == '-'
                            || character == '_'
                            || character == '.'
                            || character == '/'))
            })
            && !self.0.ends_with(['-', '_', '.', '/'])
            && !self.0.contains("..")
            && !self.0.contains("//")
    }
}

impl From<&str> for Category {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for Category {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
/// Severity used for a reported capability.
pub enum Severity {
    /// Informational finding.
    Info,
    /// Warning finding.
    Warning,
    /// Error finding.
    Error,
}

impl Severity {
    #[must_use]
    /// Return the stable serialized spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    /// Convert to the generic diagnostic severity type.
    pub fn as_diagnostic_severity(self) -> crate::diagnostic::Severity {
        match self {
            Self::Info => crate::diagnostic::Severity::Info,
            Self::Warning => crate::diagnostic::Severity::Warning,
            Self::Error => crate::diagnostic::Severity::Error,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
/// Confidence assigned to the semantic evidence.
pub enum Confidence {
    /// Strongly proven identity/flow.
    High,
    /// Partially constrained but supported identity/flow.
    Medium,
    /// Lower-confidence supported heuristic.
    Low,
}

impl Confidence {
    #[must_use]
    /// Return the stable serialized spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}
