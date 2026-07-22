//! Errors returned while building rules and validating matcher declarations.

use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Construction-time rule metadata or matcher validation failure.
pub enum RuleBuildError {
    /// Rule ID was not supplied.
    MissingId,
    /// Rule ID failed the canonical naming policy.
    InvalidId(String),
    /// Human-readable label was not supplied.
    MissingDescription,
    /// Category was not supplied.
    MissingCategory,
    /// Severity was not supplied.
    MissingSeverity,
    /// Confidence was not supplied.
    MissingConfidence,
    /// A required metadata field was supplied more than once.
    DuplicateField(&'static str),
    /// Category failed taxonomy validation.
    InvalidCategory(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatcherBuildError {
    InvalidModuleSpecifier(String),
    EmptyChain,
    InvalidArgumentIndex(usize),
    MissingRequired,
    ConflictingProvenance,
    #[doc(hidden)]
    Generic(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Catalog-level rule identity failure.
pub enum CompiledCatalogError {
    /// Two rules declared the same stable ID.
    DuplicateRule(String),
    /// A rule declaration could not be lowered into a semantic query.
    InvalidMatcher(String),
}

impl fmt::Display for RuleBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingId => formatter.write_str("rule ID is required"),
            Self::InvalidId(value) => write!(formatter, "invalid rule ID `{value}`"),
            Self::MissingDescription => formatter.write_str("rule label is required"),
            Self::MissingCategory => formatter.write_str("rule category is required"),
            Self::MissingSeverity => formatter.write_str("rule severity is required"),
            Self::MissingConfidence => formatter.write_str("rule confidence is required"),
            Self::DuplicateField(field) => {
                write!(formatter, "rule {field} was supplied more than once")
            }
            Self::InvalidCategory(value) => write!(formatter, "invalid rule category `{value}`"),
        }
    }
}

impl Error for RuleBuildError {}

impl fmt::Display for MatcherBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidModuleSpecifier(value) => {
                write!(formatter, "invalid module specifier `{value}`")
            }
            Self::EmptyChain => formatter.write_str("member chain must not be empty"),
            Self::InvalidArgumentIndex(index) => {
                write!(formatter, "argument index {index} exceeds maximum")
            }
            Self::MissingRequired => formatter.write_str("required field is missing"),
            Self::ConflictingProvenance => formatter.write_str("conflicting provenance modes"),
            Self::Generic(value) => formatter.write_str(value),
        }
    }
}

impl Error for MatcherBuildError {}

impl From<String> for MatcherBuildError {
    fn from(value: String) -> Self {
        Self::Generic(value)
    }
}

impl From<&str> for MatcherBuildError {
    fn from(value: &str) -> Self {
        Self::Generic(value.to_owned())
    }
}

impl fmt::Display for CompiledCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateRule(id) => write!(formatter, "duplicate rule `{id}`"),
            Self::InvalidMatcher(message) => write!(formatter, "invalid matcher: {message}"),
        }
    }
}

impl Error for CompiledCatalogError {}
