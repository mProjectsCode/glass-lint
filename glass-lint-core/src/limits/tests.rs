use super::*;

#[test]
fn every_analysis_limit_rejects_zero() {
    let defaults = AnalysisLimits::default();
    for (variant, zero_fn) in [
        (
            AnalysisLimitError::SyntaxDepth,
            AnalysisLimits::with_syntax_depth as fn(_, _) -> _,
        ),
        (
            AnalysisLimitError::SemanticOperations,
            AnalysisLimits::with_semantic_operations,
        ),
        (
            AnalysisLimitError::EffectOperations,
            AnalysisLimits::with_effect_operations,
        ),
        (
            AnalysisLimitError::EvidenceItems,
            AnalysisLimits::with_evidence_items,
        ),
        (
            AnalysisLimitError::LinkOperations,
            AnalysisLimits::with_link_operations,
        ),
        (
            AnalysisLimitError::FlowOperations,
            AnalysisLimits::with_flow_operations,
        ),
        (
            AnalysisLimitError::TraceNodes,
            AnalysisLimits::with_trace_nodes,
        ),
    ] {
        assert_eq!(zero_fn(defaults.clone(), 0), Err(variant));
    }
}

#[test]
fn project_admission_limits_reject_zero_and_expose_defaults() {
    assert_eq!(
        ProjectAdmissionLimits::default().max_sources(),
        DEFAULT_MAX_PROJECT_SOURCES
    );
    assert_eq!(
        ProjectAdmissionLimits::default().max_source_bytes(),
        DEFAULT_MAX_PROJECT_SOURCE_BYTES
    );
    assert_eq!(
        ProjectAdmissionLimits::new(0, 1),
        Err(ProjectAdmissionLimitError::MaxSources)
    );
    assert_eq!(
        ProjectAdmissionLimits::new(1, 0),
        Err(ProjectAdmissionLimitError::MaxSourceBytes)
    );
}

#[test]
fn accessors_return_configured_values() {
    let limits = AnalysisLimits::default()
        .with_syntax_depth(10)
        .and_then(|limits| limits.with_semantic_operations(20))
        .and_then(|limits| limits.with_effect_operations(30))
        .and_then(|limits| limits.with_evidence_items(40))
        .and_then(|limits| limits.with_link_operations(50))
        .and_then(|limits| limits.with_flow_operations(60))
        .and_then(|limits| limits.with_trace_nodes(70))
        .unwrap();
    assert_eq!(limits.syntax_depth(), 10);
    assert_eq!(limits.semantic_operations(), 20);
    assert_eq!(limits.effect_operations(), 30);
    assert_eq!(limits.evidence_items(), 40);
    assert_eq!(limits.link_operations(), 50);
    assert_eq!(limits.flow_operations(), 60);
    assert_eq!(limits.trace_nodes(), 70);
}

#[cfg(feature = "serde")]
#[test]
fn deserialization_rejects_zero() {
    let json = r#"{"syntax_depth":0}"#;
    let result: Result<AnalysisLimits, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[cfg(feature = "serde")]
#[test]
fn deserialization_accepts_partial_with_defaults() {
    let json = r#"{"syntax_depth":256}"#;
    let limits: AnalysisLimits = serde_json::from_str(json).unwrap();
    assert_eq!(limits.syntax_depth(), 256);
    assert_eq!(limits.semantic_operations(), default_semantic_operations());
}
