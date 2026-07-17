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
    /// No valid matcher survived normalization.
    MissingMatcher,
    /// Category failed taxonomy validation.
    InvalidCategory(String),
    /// A matcher failed shape/provenance validation.
    InvalidMatcher(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Catalog-level rule identity failure.
pub enum CompiledCatalogError {
    /// Two rules declared the same stable ID.
    DuplicateRule(String),
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
            Self::MissingMatcher => formatter.write_str("at least one matcher is required"),
            Self::InvalidCategory(value) => write!(formatter, "invalid rule category `{value}`"),
            Self::InvalidMatcher(value) => write!(formatter, "invalid matcher: {value}"),
        }
    }
}

impl Error for RuleBuildError {}

impl fmt::Display for CompiledCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateRule(id) => write!(formatter, "duplicate rule `{id}`"),
        }
    }
}

impl Error for CompiledCatalogError {}
