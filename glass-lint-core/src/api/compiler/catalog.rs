//! Rule catalog construction and duplicate-ID validation.

use crate::{
    Rule,
    api::{
        compiler::CompiledRuleRecord,
        rule::{CompiledCatalogError, MatcherBuildError, QueryDiagnostic},
    },
};

/// Compile rules into records in deterministic declaration order.
pub(crate) fn compile_records(
    rules: &[Rule],
) -> Result<Vec<CompiledRuleRecord>, CompiledCatalogError> {
    rules
        .iter()
        .map(|rule| {
            CompiledRuleRecord::new(rule).map_err(|e| match e {
                MatcherBuildError::QueryCompileError(diagnostic) => {
                    CompiledCatalogError::InvalidQuery {
                        rule_id: rule.id().to_owned(),
                        diagnostic,
                    }
                }
                MatcherBuildError::QueryBuildError(qbe) => CompiledCatalogError::InvalidQuery {
                    rule_id: rule.id().to_owned(),
                    diagnostic: QueryDiagnostic {
                        code: "query_build_error",
                        message: qbe.to_string(),
                    },
                },
                MatcherBuildError::CompilerInvariant(message) => {
                    CompiledCatalogError::CompilerInvariant {
                        rule_id: rule.id().to_owned(),
                        message,
                    }
                }
                MatcherBuildError::InvalidPhysicalPlan(message) => {
                    CompiledCatalogError::InvalidPhysicalPlan {
                        rule_id: rule.id().to_owned(),
                        message,
                    }
                }
                _ => CompiledCatalogError::InvalidMatcher {
                    rule_id: rule.id().to_owned(),
                    message: e.to_string(),
                },
            })
        })
        .collect::<Result<Vec<_>, _>>()
}
