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
        DiagnosticKind::NameBudgetExhausted,
        DiagnosticKind::PathCapacityExhausted,
        DiagnosticKind::RuleSelectionInvalid,
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
