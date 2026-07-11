//! Generic, provenance-aware JavaScript linting.

use std::{collections::BTreeMap, error::Error, fmt};

use serde::{Deserialize, Serialize};
use swc_common::{FileName, SourceMap, Spanned, sync::Lrc};
use swc_ecma_ast::{EsVersion, Program};
use swc_ecma_parser::{EsSyntax, Parser, StringInput, Syntax, lexer::Lexer};

mod linter;
mod matcher;

pub use linter::{Linter, RuleCatalog};

/// Declarative rule-building API for provider crates and custom catalogs.
pub mod rules {
    /// Builders for strict, provenance-aware matchers.
    ///
    /// Constructors named `global`, `rooted_chain`, and `module_*` require
    /// semantic proof.  `heuristic_*` constructors intentionally opt into
    /// weaker syntactic matching and should be used only by an explicit
    /// heuristic profile.
    pub use crate::matcher::{
        ApiCategory as Category, ApiRule as Rule, ApiRuleBuildError as BuildError,
        ApiRuleBuilder as Builder, ApiSeverity as Severity, CallMatcher, ClassMatcher, Confidence,
        ConstructorMatcher, FlowMatcher, FlowValueMatcher, InstanceMemberCallMatcher, Matcher,
        MemberCallMatcher, MemberReadMatcher, ReturnedMemberCallMatcher, ReturnedMemberReadMatcher,
    };
}

pub const REPORT_VERSION: u32 = 2;
/// Inputs above this bound receive a controlled parse diagnostic.  The core
/// is intended for source files, not unbounded data blobs or hostile bundles.
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct RuleId(String);

impl RuleId {
    pub fn parse(value: impl Into<String>) -> Result<Self, RegistryError> {
        let value = value.into();
        let Some((provider, name)) = value.split_once(':') else {
            return Err(RegistryError::InvalidRuleId(value));
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
            return Err(RegistryError::InvalidRuleId(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RuleId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceRange {
    pub start: Position,
    pub end: Position,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Evidence {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<SourceRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Finding {
    pub rule_id: RuleId,
    pub message_id: String,
    pub message: String,
    pub severity: Severity,
    pub range: SourceRange,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ParseDiagnostic {
    pub message: String,
    pub filename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<SourceRange>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LintReport {
    pub schema_version: u32,
    pub tool_version: String,
    pub findings: Vec<Finding>,
    pub parse_diagnostics: Vec<ParseDiagnostic>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RuleMetadata {
    pub id: RuleId,
    pub description: String,
    pub default_severity: Severity,
    #[serde(default)]
    pub messages: BTreeMap<String, String>,
}

pub(crate) struct ParsedSource {
    pub program: Program,
    pub source_map: Lrc<SourceMap>,
}

pub(crate) fn parse(source: &str, filename: &str) -> Result<ParsedSource, ParseDiagnostic> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(ParseDiagnostic {
            message: format!(
                "JavaScript source exceeds the {} byte analysis limit",
                MAX_SOURCE_BYTES
            ),
            filename: filename.to_string(),
            range: None,
        });
    }
    let source_map = Lrc::new(SourceMap::default());
    let file =
        source_map.new_source_file(FileName::Custom(filename.into()).into(), source.to_owned());
    let lexer = Lexer::new(
        Syntax::Es(EsSyntax {
            jsx: true,
            decorators: true,
            fn_bind: true,
            export_default_from: true,
            import_attributes: true,
            allow_super_outside_method: true,
            allow_return_outside_function: true,
            auto_accessors: true,
            explicit_resource_management: true,
            ..Default::default()
        }),
        EsVersion::EsNext,
        StringInput::from(&*file),
        None,
    );
    Parser::new_from(lexer)
        .parse_program()
        .map(|program| ParsedSource {
            program,
            source_map: source_map.clone(),
        })
        .map_err(|error| ParseDiagnostic {
            message: format!("JavaScript parse error: {}", error.kind().msg()),
            filename: filename.to_string(),
            range: (!error.span().is_dummy()).then(|| {
                let start = source_map.lookup_char_pos(error.span().lo());
                let end = source_map.lookup_char_pos(error.span().hi());
                SourceRange {
                    start: Position {
                        line: u32::try_from(start.line).unwrap_or(u32::MAX),
                        column: u32::try_from(start.col_display)
                            .unwrap_or(u32::MAX)
                            .saturating_add(1),
                    },
                    end: Position {
                        line: u32::try_from(end.line).unwrap_or(u32::MAX),
                        column: u32::try_from(end.col_display)
                            .unwrap_or(u32::MAX)
                            .saturating_add(1),
                    },
                }
            }),
        })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    InvalidRuleId(String),
    InvalidRule(String, String),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRuleId(id) => write!(formatter, "invalid rule ID `{id}`"),
            Self::InvalidRule(id, message) => write!(formatter, "invalid rule `{id}`: {message}"),
        }
    }
}

impl Error for RegistryError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LintConfigError {
    UnknownRule(RuleId),
}

impl fmt::Display for LintConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRule(id) => write!(formatter, "unknown rule `{id}`"),
        }
    }
}

impl Error for LintConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_namespaced_rule_ids() {
        assert!(RuleId::parse("provider:network.fetch").is_ok());
        assert!(RuleId::parse("missing_namespace").is_err());
    }
}
