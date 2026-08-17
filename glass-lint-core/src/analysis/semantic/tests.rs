use swc_common::{BytePos, Span};

use super::*;
use crate::project::ProjectRelativePath;

#[test]
fn swc_span_is_normalized_to_zero_based_byte_range_once() {
    let normalizer = SpanNormalizer::new(BytePos(40), &SourceText::from("aé\r\n"));
    assert_eq!(
        normalizer.normalize(Span::new(BytePos(40), BytePos(43))),
        Ok(glass_lint_datastructures::ByteRange::new(0, 3).unwrap())
    );
    assert!(
        normalizer
            .normalize(Span::new(BytePos(42), BytePos(43)))
            .is_err()
    );
    assert!(
        normalizer
            .normalize(Span::new(BytePos(40), BytePos(46)))
            .is_err()
    );
}

#[test]
fn name_exhaustion_invalidates_indexes_and_effects_with_an_accurate_status() {
    let source = "function helper(options) { return options.send; } helper({ send: 1 });";
    let parsed =
        crate::parse_test_source(source, "name-exhaustion.js").expect("source should parse");
    let coordinates = SpanNormalizer::new(parsed.source_start, &SourceText::from(source));
    let environment = crate::Environment::default();
    let limits = crate::AnalysisLimits::default();
    let analyzer = SemanticAnalyzer::new(&environment, &limits);
    let artifact = analyzer.analyze_program_with_name_limit(&parsed.program, &coordinates, 2);

    assert!(!artifact.facts().stream().is_valid());
    assert!(!artifact.facts().is_projectable());
    assert!(artifact.facts().matcher_index().is_empty());
    assert!(!artifact.facts().matcher_index().is_available());
    assert!(artifact.effects().iter_effects().next().is_none());
    assert!(!artifact.effects().is_available());
    let (file_diagnostics, project_diagnostics) = artifact
        .status()
        .materialize_file(&ProjectRelativePath::new("name-exhaustion.js").unwrap())
        .diagnostics();
    assert_eq!(project_diagnostics.len(), 0);
    assert_eq!(file_diagnostics.len(), 1);
    assert_eq!(
        file_diagnostics[0].1.code().as_str(),
        "semantic_name_budget_exhausted"
    );
    assert!(file_diagnostics[0].1.message().contains("limit=2"));
    assert!(file_diagnostics[0].1.message().contains("attempted=3"));

    let repeated = analyzer.analyze_program_with_name_limit(&parsed.program, &coordinates, 2);
    assert_eq!(
        format!("{:?}", artifact.facts().stream().facts()),
        format!("{:?}", repeated.facts().stream().facts())
    );
    assert_eq!(artifact.status(), repeated.status());
}

#[test]
fn scope_shape_failure_disables_derived_phases() {
    let mut completion = AnalysisCompletion::new();
    completion.record_scope_issue(1);

    assert!(!completion.capabilities.availability().is_enabled());
    assert!(!completion.status.is_complete());
}

#[test]
fn tiny_semantic_budget_stops_traversal_and_skips_derived_phases() {
    let source = "
            function helper(a, b) { return a + b; }
            function process(c, d) { return helper(c, d); }
            function compute(e, f) { return process(e, f); }
            export const result = compute(1, 2);
            export function identity(x) { return x; }
        ";
    let parsed =
        crate::parse_test_source(source, "budget-exhaustion.js").expect("source should parse");
    let coordinates = SpanNormalizer::new(parsed.source_start, &SourceText::from(source));

    let limits = crate::AnalysisLimits::default()
        .with_semantic_operations(10)
        .expect("valid limit");
    let artifact = SemanticAnalyzer::new(&crate::Environment::default(), &limits)
        .analyze_program(&parsed.program, &coordinates);

    assert!(!artifact.status().is_complete());
    assert!(artifact.effects().iter_effects().next().is_none());
    assert!(!artifact.effects().is_available());
    // With budget of 10, the fact stream has very few facts
    assert!(artifact.facts().stream().facts().len() < 5);
    assert_eq!(artifact.facts().stream().max_facts(), MAX_FACTS);
    // Export origin lookups return nothing since the phase was skipped
    assert!(artifact.export_origin("result").is_none());
    assert!(artifact.export_origin("identity").is_none());
}

#[test]
fn large_semantic_budget_produces_complete_artifact_with_export_origins() {
    let source = "
            function helper(a, b) { return a + b; }
            function process(c, d) { return helper(c, d); }
            function compute(e, f) { return process(e, f); }
            export const result = compute(1, 2);
            export function identity(x) { return x; }
        ";
    let parsed =
        crate::parse_test_source(source, "budget-sufficient.js").expect("source should parse");
    let coordinates = SpanNormalizer::new(parsed.source_start, &SourceText::from(source));

    let artifact = SemanticAnalyzer::new(
        &crate::Environment::default(),
        &crate::AnalysisLimits::default(),
    )
    .analyze_program(&parsed.program, &coordinates);

    assert!(artifact.status().is_complete());
    assert!(artifact.facts().stream().facts().len() > 10);
    assert!(artifact.effects().iter_effects().next().is_some());
    assert!(artifact.facts().matcher_index().is_available());
    assert!(artifact.effects().is_available());
    // Export origins should be present since the phase ran
    assert!(artifact.export_origin("result").is_some());
    assert!(artifact.export_origin("identity").is_some());
}

#[test]
fn invalid_parser_span_records_incomplete_at_file_location() {
    let source = "fetch('/remote');";
    let parsed = crate::parse_test_source(source, "main.js").unwrap();
    let invalid = SpanNormalizer::new(
        BytePos(parsed.source_start.0 + 100),
        &SourceText::from(source),
    );
    let artifact = SemanticAnalyzer::new(
        &crate::Environment::default(),
        &crate::AnalysisLimits::default(),
    )
    .analyze_program(&parsed.program, &invalid);
    assert!(!artifact.status().is_complete());
    assert!(artifact.facts().stream().facts().is_empty());
    let (files, project) = artifact
        .status()
        .materialize_file(&ProjectRelativePath::new("main.js").unwrap())
        .diagnostics();
    assert_eq!(files.len(), 1);
    assert_eq!(project.len(), 0);
    assert_eq!(files[0].1.code().as_str(), "invalid_parser_span");
    assert_eq!(
        files[0]
            .1
            .location()
            .map(|location| location.path().as_str()),
        Some("main.js")
    );
}
