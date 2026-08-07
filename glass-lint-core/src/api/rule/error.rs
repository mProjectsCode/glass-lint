//! Errors returned while building rules and validating matcher declarations.

use std::{error::Error, fmt};

use super::query::QueryDiagnostic;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Construction-time rule metadata or matcher validation failure.
pub enum RuleBuildError {
    /// Rule ID was not supplied.
    MissingId,
    /// Rule ID failed the canonical naming policy.
    InvalidId(String),
    /// Human-readable label was not supplied.
    MissingDescription,
    /// Severity was not supplied.
    MissingSeverity,
    /// Confidence was not supplied.
    MissingConfidence,
    /// At least one query declaration is required.
    MissingQuery,
    /// A required metadata field was supplied more than once.
    DuplicateField(&'static str),
    /// A query declaration could not be constructed.
    InvalidQuery(super::query::QueryBuildError),
    /// The rule contains more query roots than the bounded authoring limit.
    TooManyQueries(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatcherBuildError {
    InvalidModuleSpecifier(String),
    /// A compiler invariant failed after authored query validation.
    CompilerInvariant(String),
    /// A normalized query could not form a valid physical plan.
    InvalidPhysicalPlan(String),
    /// An authored query compilation error with a stable structured diagnostic.
    QueryCompileError(QueryDiagnostic),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Catalog-level rule identity failure.
pub enum CompiledCatalogError {
    /// A rule declaration could not be lowered into a semantic query.
    InvalidMatcher { rule_id: String, message: String },
    /// A rule query failed compilation with a structured diagnostic.
    InvalidQuery {
        rule_id: String,
        diagnostic: QueryDiagnostic,
    },
    /// A compiler invariant failed while compiling a rule.
    CompilerInvariant { rule_id: String, message: String },
    /// A normalized query could not form a valid physical plan.
    InvalidPhysicalPlan { rule_id: String, message: String },
}

impl fmt::Display for RuleBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingId => formatter.write_str("rule ID is required"),
            Self::InvalidId(value) => write!(formatter, "invalid rule ID `{value}`"),
            Self::MissingDescription => formatter.write_str("rule label is required"),
            Self::MissingSeverity => formatter.write_str("rule severity is required"),
            Self::MissingConfidence => formatter.write_str("rule confidence is required"),
            Self::MissingQuery => formatter.write_str("rule requires at least one query"),
            Self::DuplicateField(field) => {
                write!(formatter, "rule {field} was supplied more than once")
            }
            Self::InvalidQuery(err) => write!(formatter, "invalid query: {err}"),
            Self::TooManyQueries(count) => {
                write!(
                    formatter,
                    "rule contains {count} query roots, exceeding the limit"
                )
            }
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
            Self::CompilerInvariant(msg) => {
                write!(formatter, "compiler invariant failure: {msg}")
            }
            Self::InvalidPhysicalPlan(msg) => {
                write!(formatter, "invalid physical plan: {msg}")
            }
            Self::QueryCompileError(e) => write!(formatter, "{e}"),
        }
    }
}

impl Error for MatcherBuildError {}

impl fmt::Display for CompiledCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMatcher { rule_id, message } => {
                write!(formatter, "rule `{rule_id}`: invalid matcher: {message}")
            }
            Self::InvalidQuery {
                rule_id,
                diagnostic,
            } => write!(formatter, "rule `{rule_id}`: {diagnostic}"),
            Self::CompilerInvariant { rule_id, message } => {
                write!(
                    formatter,
                    "rule `{rule_id}`: compiler invariant failure: {message}"
                )
            }
            Self::InvalidPhysicalPlan { rule_id, message } => {
                write!(
                    formatter,
                    "rule `{rule_id}`: invalid physical plan: {message}"
                )
            }
        }
    }
}

impl Error for CompiledCatalogError {}
