//! Errors returned while building rules and validating matcher declarations.

use std::{error::Error, fmt};

use super::query::{QueryDiagnostic, limits};

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

/// Structured failure of an internal compiler invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompilerInvariantDiagnostic {
    /// Same-event normalization could not find a required merged component.
    IncompleteSameEvent { missing: String },
    /// A normalized representation violated an internal compiler invariant.
    Internal { detail: String },
}

impl fmt::Display for CompilerInvariantDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncompleteSameEvent { missing } => {
                write!(formatter, "same-event merge missing {missing}")
            }
            Self::Internal { detail } => formatter.write_str(detail),
        }
    }
}

/// Structured validation failure for an executable physical matcher plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhysicalPlanDiagnostic {
    /// No supported identity/event/subject combination can be selected.
    ImpossibleDimensions,
    /// Argument constraints were attached to a non-call event.
    ConstraintsRequireCallEvent,
    /// Constraints were not in canonical grouped order.
    NonCanonicalConstraints,
    /// The physical root has no primary evidence symbol.
    UnavailablePrimaryEvidence,
    /// A lifecycle root has no usable source.
    InvalidLifecycleRoot,
    /// A lifecycle source failed semantic validation.
    InvalidLifecycleSource { detail: String },
    /// Lifecycle evidence exceeded its indexed bound.
    ExcessiveLifecycleEvidence { requirements: usize, sinks: usize },
    /// Argument groups exceeded the configured bound.
    ExcessiveArgumentGroups(usize),
    /// Predicates for one argument exceeded the configured bound.
    ExcessivePredicateCount(usize),
    /// Static alternatives exceeded the configured bound.
    ExcessiveAlternatives(usize),
}

impl fmt::Display for PhysicalPlanDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ImpossibleDimensions => formatter
                .write_str("identity/event/subject dimensions cannot select a semantic fact"),
            Self::ConstraintsRequireCallEvent => {
                formatter.write_str("argument constraints require a call-bearing event")
            }
            Self::NonCanonicalConstraints => {
                formatter.write_str("constraints are not in canonical grouped order")
            }
            Self::UnavailablePrimaryEvidence => {
                formatter.write_str("primary evidence symbol is empty")
            }
            Self::InvalidLifecycleRoot => formatter.write_str("lifecycle root is malformed"),
            Self::InvalidLifecycleSource { detail } => formatter.write_str(detail),
            Self::ExcessiveLifecycleEvidence {
                requirements,
                sinks,
            } => write!(
                formatter,
                "lifecycle evidence has {requirements} requirements and {sinks} sinks, exceeding the indexed bound"
            ),
            Self::ExcessiveArgumentGroups(count) => write!(
                formatter,
                "argument group count {count} exceeds limit {}",
                limits::MAX_ARGUMENT_GROUPS
            ),
            Self::ExcessivePredicateCount(count) => write!(
                formatter,
                "predicate count {count} exceeds limit {}",
                limits::MAX_PREDICATES_PER_ARGUMENT
            ),
            Self::ExcessiveAlternatives(count) => write!(
                formatter,
                "static alternative count {count} exceeds limit {}",
                limits::MAX_STATIC_ALTERNATIVES
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatcherBuildError {
    /// A module specifier failed package-boundary validation.
    InvalidModuleSpecifier(String),
    /// A compiler invariant failed after authored query validation.
    CompilerInvariant(CompilerInvariantDiagnostic),
    /// A normalized query could not form a valid physical plan.
    InvalidPhysicalPlan(PhysicalPlanDiagnostic),
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
    CompilerInvariant {
        rule_id: String,
        diagnostic: CompilerInvariantDiagnostic,
    },
    /// A normalized query could not form a valid physical plan.
    InvalidPhysicalPlan {
        rule_id: String,
        diagnostic: PhysicalPlanDiagnostic,
    },
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
            Self::CompilerInvariant(diagnostic) => {
                write!(formatter, "compiler invariant failure: {diagnostic}")
            }
            Self::InvalidPhysicalPlan(diagnostic) => {
                write!(formatter, "invalid physical plan: {diagnostic}")
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
            Self::CompilerInvariant {
                rule_id,
                diagnostic,
            } => {
                write!(
                    formatter,
                    "rule `{rule_id}`: compiler invariant failure: {diagnostic}"
                )
            }
            Self::InvalidPhysicalPlan {
                rule_id,
                diagnostic,
            } => {
                write!(
                    formatter,
                    "rule `{rule_id}`: invalid physical plan: {diagnostic}"
                )
            }
        }
    }
}

impl Error for CompiledCatalogError {}
