use std::fmt;

use super::limits;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryBuildError {
    EmptyAlternatives,
    EmptyConjunction,
    EmptyIdentityName,
    EmptyModuleSpecifier,
    EmptyStaticValue,
    EmptyEvidenceSymbol,
    MalformedChain(String),
    InvalidArgumentIndex(usize),
    InvalidScopePackage,
    ExcessiveConstraints(usize),
    ExcessiveArgumentGroups(usize),
    ExcessivePredicates { index: usize, count: usize },
    EmptyCollection(&'static str),
    MissingLifecycleSources,
    MissingLifecycleCompletion,
    EmptyLifecycleCondition,
    EmptyLifecycleSinks,
    MissingLifecycleCondition,
    CollectionTooLarge(&'static str, usize),
    ExpressionDepthExceeded(usize),
    EvidenceProjection,
    DuplicateLifecycleStage(&'static str),
}

impl fmt::Display for QueryBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyAlternatives => write!(f, "Any expression must have at least one branch"),
            Self::EmptyConjunction => write!(f, "All expression must have at least one branch"),
            Self::EmptyIdentityName => write!(f, "identity name must not be empty"),
            Self::EmptyModuleSpecifier => write!(f, "module specifier must not be empty"),
            Self::EmptyStaticValue => write!(f, "static value must not be empty"),
            Self::EmptyEvidenceSymbol => write!(f, "evidence symbol must not be empty"),
            Self::MalformedChain(chain) => write!(f, "malformed member chain: {chain}"),
            Self::InvalidArgumentIndex(idx) => write!(
                f,
                "argument index {idx} exceeds maximum ({})",
                limits::MAX_ARGUMENT_INDEX
            ),
            Self::InvalidScopePackage => write!(f, "invalid package scope pattern"),
            Self::ExcessiveConstraints(count) => {
                write!(f, "constraint count {count} exceeds limit")
            }
            Self::ExcessiveArgumentGroups(count) => {
                write!(f, "argument group count {count} exceeds maximum")
            }
            Self::ExcessivePredicates { index, count } => write!(
                f,
                "argument {index} has {count} predicates, exceeding maximum"
            ),
            Self::EmptyCollection(name) => write!(f, "{name} must not be empty"),
            Self::MissingLifecycleSources => write!(f, "lifecycle requires at least one source"),
            Self::MissingLifecycleCompletion => {
                write!(f, "lifecycle requires exactly one completion")
            }
            Self::EmptyLifecycleCondition => {
                write!(f, "lifecycle condition requires at least one event")
            }
            Self::EmptyLifecycleSinks => {
                write!(f, "lifecycle sink completion requires at least one sink")
            }
            Self::MissingLifecycleCondition => {
                write!(f, "configuration completion requires a condition")
            }
            Self::CollectionTooLarge(name, size) => {
                write!(f, "{name} size {size} exceeds maximum")
            }
            Self::ExpressionDepthExceeded(depth) => write!(
                f,
                "expression depth {depth} exceeds maximum ({})",
                limits::MAX_EXPR_DEPTH
            ),
            Self::DuplicateLifecycleStage(stage) => {
                write!(f, "lifecycle {stage} may only be specified once")
            }
            Self::EvidenceProjection => {
                f.write_str("query alternatives have incompatible evidence projections")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

impl QueryDiagnostic {
    /// Construct a diagnostic at an internal compiler boundary.
    pub(crate) fn new(code: &'static str, message: String) -> Self {
        Self { code, message }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for QueryDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code(), self.message())
    }
}
