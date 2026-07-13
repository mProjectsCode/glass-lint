//! Generic, provenance-aware JavaScript linting.

use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

mod analysis;
mod api;
mod diagnostic;
mod lint;
mod parse;

pub use api::rule::{ApiRule as Rule, ApiRuleBuildError as BuildError};
pub use diagnostic::{
    Evidence, Finding, LintReport, Position, RuleMetadata, Severity, SourceRange,
};
pub use lint::{LintConfigError, Linter, RuleCatalog, RuleCatalogError};
pub use parse::ParseDiagnostic;
#[allow(unused_imports)]
pub(crate) use parse::parse;

pub const REPORT_VERSION: u32 = 2;
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct RuleId(String);

impl RuleId {
    pub fn parse(value: impl Into<String>) -> Result<Self, RuleCatalogError> {
        let value = value.into();
        let Some((provider, name)) = value.split_once(':') else {
            return Err(RuleCatalogError::InvalidRuleId(value));
        };
        let valid_part = |part: &str, allow_dot: bool| {
            !part.is_empty()
                && part.chars().enumerate().all(|(index, character)| {
                    (index > 0 && character.is_ascii_digit())
                        || character.is_ascii_lowercase()
                        || character == '-'
                        || character == '_'
                        || (allow_dot && character == '.')
                })
                && !part.starts_with(['-', '_', '.'])
                && !part.ends_with(['-', '_', '.'])
                && !part.contains("..")
        };
        if !valid_part(provider, false) || !valid_part(name, true) {
            return Err(RuleCatalogError::InvalidRuleId(value));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for RuleId {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::parse(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}
impl Error for RuleId {}

/// Declarative rule-building API for provider crates and custom catalogs.
pub mod rules {
    pub use crate::api::rule::{
        ApiCategory as Category, ApiRule as Rule, ApiRuleBuildError as BuildError,
        ApiRuleBuilder as Builder, ApiSeverity as Severity, CallMatcher, ClassMatcher, Confidence,
        ConstructorMatcher, FlowMatcher, FlowValueMatcher, InstanceMemberCallMatcher, Matcher,
        MemberCallMatcher, MemberReadMatcher, ReturnedMemberCallMatcher, ReturnedMemberReadMatcher,
    };
}
