#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct DiagnosticCode(String);

const MAX_DIAGNOSTIC_CODE_LEN: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticKind {
    AmbiguousStarExport,
    EffectsBudgetExhausted,
    FactCapacityExhausted,
    FlowBudgetExhausted,
    LinkingBudgetExhausted,
    InvalidParserSpan,
    MissingImportedExport,
    OutsideProjectTarget,
    FactsBudgetExhausted,
    NameBudgetExhausted,
    PathCapacityExhausted,
    ScopeShapeMismatch,
    SemanticBudgetExhausted,
    SourceTooLarge,
    SyntaxDepthExceeded,
    SyntaxError,
    UnresolvedInternalRequest,
    UnsupportedCommonjsExports,
    UnsupportedProjectTarget,
    ValueArenaExhausted,
}

impl DiagnosticKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AmbiguousStarExport => "ambiguous_star_export",
            Self::EffectsBudgetExhausted => "effect_size_budget_exhausted",
            Self::FactCapacityExhausted => "semantic_fact_capacity_exhausted",
            Self::FlowBudgetExhausted => "flow_link_budget_exhausted",
            Self::LinkingBudgetExhausted => "graph_link_budget_exhausted",
            Self::InvalidParserSpan => "invalid_parser_span",
            Self::MissingImportedExport => "missing_imported_export",
            Self::OutsideProjectTarget => "outside_project_target",
            Self::FactsBudgetExhausted => "semantic_budget_exhausted",
            Self::NameBudgetExhausted => "semantic_name_budget_exhausted",
            Self::PathCapacityExhausted => "semantic_path_capacity_exhausted",
            Self::ScopeShapeMismatch => "scope_shape_mismatch",
            Self::SemanticBudgetExhausted => "semantic_step_budget_exhausted",
            Self::SourceTooLarge => "source_too_large",
            Self::SyntaxDepthExceeded => "syntax_depth_exceeded",
            Self::SyntaxError => "syntax_error",
            Self::UnresolvedInternalRequest => "unresolved_internal_request",
            Self::UnsupportedCommonjsExports => "unsupported_commonjs_exports",
            Self::UnsupportedProjectTarget => "unsupported_project_target",
            Self::ValueArenaExhausted => "semantic_value_arena_exhausted",
        }
    }
}

impl DiagnosticCode {
    pub fn new(code: impl Into<String>) -> Result<Self, String> {
        let code = code.into();
        if !code.is_empty()
            && code.len() <= MAX_DIAGNOSTIC_CODE_LEN
            && code.chars().all(|character| {
                character.is_ascii_lowercase() || character == '_' || character.is_ascii_digit()
            })
            && code.as_bytes()[0].is_ascii_lowercase()
        {
            Ok(Self(code))
        } else {
            Err(code)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for DiagnosticCode {
    type Error = String;

    fn try_from(code: String) -> Result<Self, Self::Error> {
        Self::new(code)
    }
}

impl TryFrom<&str> for DiagnosticCode {
    type Error = String;

    fn try_from(code: &str) -> Result<Self, Self::Error> {
        Self::new(code)
    }
}

impl From<DiagnosticKind> for DiagnosticCode {
    fn from(kind: DiagnosticKind) -> Self {
        Self::new(kind.as_str()).expect("DiagnosticKind literals are canonical")
    }
}

impl std::fmt::Display for DiagnosticCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_kind_table_contains_only_canonical_codes() {
        let kinds = [
            DiagnosticKind::AmbiguousStarExport,
            DiagnosticKind::EffectsBudgetExhausted,
            DiagnosticKind::FactCapacityExhausted,
            DiagnosticKind::FlowBudgetExhausted,
            DiagnosticKind::LinkingBudgetExhausted,
            DiagnosticKind::InvalidParserSpan,
            DiagnosticKind::MissingImportedExport,
            DiagnosticKind::OutsideProjectTarget,
            DiagnosticKind::FactsBudgetExhausted,
            DiagnosticKind::NameBudgetExhausted,
            DiagnosticKind::PathCapacityExhausted,
            DiagnosticKind::ScopeShapeMismatch,
            DiagnosticKind::SemanticBudgetExhausted,
            DiagnosticKind::SourceTooLarge,
            DiagnosticKind::SyntaxDepthExceeded,
            DiagnosticKind::SyntaxError,
            DiagnosticKind::UnresolvedInternalRequest,
            DiagnosticKind::UnsupportedCommonjsExports,
            DiagnosticKind::UnsupportedProjectTarget,
            DiagnosticKind::ValueArenaExhausted,
        ];
        for kind in kinds {
            let owned: DiagnosticCode = kind.into();
            assert_eq!(DiagnosticCode::try_from(kind.as_str()), Ok(owned));
        }
    }
}
