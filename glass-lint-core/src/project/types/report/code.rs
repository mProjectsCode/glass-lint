#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct DiagnosticCode(String);

const MAX_DIAGNOSTIC_CODE_LEN: usize = 64;

macro_rules! diagnostic_kinds {
    ($( $variant:ident ),* $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum DiagnosticKind {
            $(
                $variant,
            )*
        }

        #[cfg(test)]
        const ALL: &[DiagnosticKind] = &[
            $(
                DiagnosticKind::$variant,
            )*
        ];
    };
}

diagnostic_kinds! {
    AmbiguousStarExport,
    EvidenceCapacityMismatch,
    EffectsBudgetExhausted,
    FactCapacityExhausted,
    FlowBudgetExhausted,
    IncompleteProject,
    LinkingBudgetExhausted,
    InvalidParserSpan,
    MissingImportedExport,
    OutsideProjectTarget,
    NameBudgetExhausted,
    PathCapacityExhausted,
    RuleSelectionInvalid,
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
            Self::EvidenceCapacityMismatch => "evidence_capacity_mismatch",
            Self::EffectsBudgetExhausted => "effect_size_budget_exhausted",
            Self::FactCapacityExhausted => "semantic_fact_capacity_exhausted",
            Self::FlowBudgetExhausted => "flow_link_budget_exhausted",
            Self::IncompleteProject => "incomplete_project",
            Self::LinkingBudgetExhausted => "graph_link_budget_exhausted",
            Self::InvalidParserSpan => "invalid_parser_span",
            Self::MissingImportedExport => "missing_imported_export",
            Self::OutsideProjectTarget => "outside_project_target",
            Self::NameBudgetExhausted => "semantic_name_budget_exhausted",
            Self::PathCapacityExhausted => "semantic_path_capacity_exhausted",
            Self::RuleSelectionInvalid => "rule_selection_invalid",
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
mod tests;
