use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiRuleBuildError {
    MissingId,
    InvalidId(String),
    MissingLabel,
    MissingCategory,
    MissingSeverity,
    MissingConfidence,
    MissingMatcher,
    InvalidCategory(String),
    InvalidMatcher(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiCatalogError {
    DuplicateRule(String),
}

impl fmt::Display for ApiRuleBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingId => formatter.write_str("rule ID is required"),
            Self::InvalidId(value) => write!(formatter, "invalid rule ID `{value}`"),
            Self::MissingLabel => formatter.write_str("rule label is required"),
            Self::MissingCategory => formatter.write_str("rule category is required"),
            Self::MissingSeverity => formatter.write_str("rule severity is required"),
            Self::MissingConfidence => formatter.write_str("rule confidence is required"),
            Self::MissingMatcher => formatter.write_str("at least one matcher is required"),
            Self::InvalidCategory(value) => write!(formatter, "invalid rule category `{value}`"),
            Self::InvalidMatcher(value) => write!(formatter, "invalid matcher: {value}"),
        }
    }
}

impl Error for ApiRuleBuildError {}

impl fmt::Display for ApiCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateRule(id) => write!(formatter, "duplicate rule `{id}`"),
        }
    }
}

impl Error for ApiCatalogError {}
