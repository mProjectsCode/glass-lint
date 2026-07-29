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
    /// A query declaration could not be constructed.
    InvalidQuery(super::query::QueryBuildError),
    /// The rule contains more query roots than the bounded authoring limit.
    TooManyQueries(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatcherBuildError {
    InvalidModuleSpecifier(String),
    EmptyChain,
    InvalidArgumentIndex(usize),
    /// A required field is missing (identity, event, or declaration).
    MissingRequired,
    ConflictingProvenance,
    /// Argument constraints require a call-bearing event.
    ConstraintsOnNonCallEvent,
    /// A lowered query clause failed validation.
    InvalidLoweredQuery(String),
    /// Object-flow compiled symbol is empty.
    EmptyFlowSymbol,
    /// Object-flow compiled plan has no sources.
    EmptyFlowSources,
    /// Object-flow compiled plan has no condition when
    /// all_requirements_required is set.
    MissingFlowCondition,
    /// Object flow has no source matchers.
    MissingFlowSource,
    /// Object flow has no completion mode.
    MissingFlowCompletion,
    /// A flow operation was specified more than once.
    DuplicateFlowOperation(&'static str),
    /// A query compilation error with structured diagnostic.
    #[allow(private_interfaces)]
    QueryCompileError(crate::api::compiler::validate::QueryCompileError),
    /// A query build error.
    QueryBuildError(super::query::QueryBuildError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Catalog-level rule identity failure.
pub enum CompiledCatalogError {
    /// A rule declaration could not be lowered into a semantic query.
    InvalidMatcher(String),
    /// A rule query failed compilation with a structured diagnostic.
    InvalidQuery {
        rule_id: String,
        diagnostic: QueryDiagnostic,
    },
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
            Self::EmptyChain => formatter.write_str("member chain must not be empty"),
            Self::InvalidArgumentIndex(index) => {
                write!(formatter, "argument index {index} exceeds maximum")
            }
            Self::MissingRequired => formatter.write_str("required field is missing"),
            Self::ConflictingProvenance => formatter.write_str("conflicting provenance modes"),
            Self::ConstraintsOnNonCallEvent => {
                formatter.write_str("argument constraints require a call event")
            }
            Self::InvalidLoweredQuery(msg) => {
                write!(formatter, "invalid lowered query: {msg}")
            }
            Self::EmptyFlowSymbol => formatter.write_str("object flow symbol must not be empty"),
            Self::EmptyFlowSources => formatter.write_str("object flow plan has no sources"),
            Self::MissingFlowCondition => formatter.write_str("object flow plan has no condition"),
            Self::MissingFlowSource => {
                formatter.write_str("object flow must have at least one source")
            }
            Self::MissingFlowCompletion => {
                formatter.write_str("object flow must have a completion mode")
            }
            Self::DuplicateFlowOperation(op) => write!(formatter, "duplicate flow operation: {op}"),
            Self::QueryCompileError(e) => {
                write!(formatter, "{}: {}", e.diagnostic_name(), e)
            }
            Self::QueryBuildError(e) => write!(formatter, "{e}"),
        }
    }
}

impl Error for MatcherBuildError {}

impl fmt::Display for CompiledCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMatcher(message) => write!(formatter, "invalid matcher: {message}"),
            Self::InvalidQuery {
                rule_id,
                diagnostic,
            } => write!(formatter, "rule `{rule_id}`: {diagnostic}"),
        }
    }
}

impl Error for CompiledCatalogError {}
