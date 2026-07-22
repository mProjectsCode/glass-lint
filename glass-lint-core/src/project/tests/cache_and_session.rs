use super::*;
use crate::{
    AnalysisLimits, Diagnostic, Environment, ReportCompletion, RuleBaseline, RuleSelection,
    SourceFile, SourceLanguage,
    analysis::ArtifactCacheHandle,
    project::session::{CountingExecutionObserver, InvocationCounts},
};

#[test]
fn cache_hit_attaches_only_current_path() {
    let linter = test_linter();
    let mut session = linter.begin_project("/project").unwrap();
    let observer = CountingExecutionObserver::new();
    session
        .admit_test_source(source_file("a.js", "fetch('x');"))
        .unwrap();
    session.analyze_source_counted("a.js", &observer).unwrap();
    session
        .admit_test_source(source_file("b.js", "fetch('x');"))
        .unwrap();
    session.analyze_source_counted("b.js", &observer).unwrap();
    assert_eq!(observer.invocations().hits, 1);
    let report = finish_collection(session);
    assert_eq!(
        report
            .files
            .iter()
            .map(|file| file.findings[0].location.path.as_str())
            .collect::<Vec<_>>(),
        vec!["a.js", "b.js"]
    );
}

#[test]
fn identical_successful_source_lowers_once_then_hits() {
    let linter = test_linter();
    let mut session = linter.begin_project("/project").unwrap();
    session
        .admit_test_source(source_file("main.js", "fetch('/api');"))
        .unwrap();
    let observer = CountingExecutionObserver::new();
    session
        .analyze_source_counted("main.js", &observer)
        .unwrap();
    session
        .analyze_source_counted("main.js", &observer)
        .unwrap();
    assert_eq!(
        observer.invocations(),
        InvocationCounts {
            parses: 1,
            lowers: 1,
            hits: 1,
            misses: 1,
            inserts: 1,
            evictions: 0,
        }
    );
    assert_eq!(finish_collection(session).files[0].findings.len(), 1);
}

#[test]
fn separate_sessions_on_one_linter_reuse_the_artifact_cache() {
    let linter = test_linter();
    let first_observer = CountingExecutionObserver::new();
    let mut first = linter.begin_project("/project").unwrap();
    first
        .admit_test_source(source_file("first.js", "fetch('/api');"))
        .unwrap();
    first
        .analyze_source_counted("first.js", &first_observer)
        .unwrap();
    finish_collection(first);

    let second_observer = CountingExecutionObserver::new();
    let mut second = linter.begin_project("/project").unwrap();
    second
        .admit_test_source(source_file("second.js", "fetch('/api');"))
        .unwrap();
    second
        .analyze_source_counted("second.js", &second_observer)
        .unwrap();
    let report = finish_collection(second);

    assert_eq!(first_observer.invocations().lowers, 1);
    assert_eq!(second_observer.invocations().hits, 1);
    assert_eq!(second_observer.invocations().lowers, 0);
    assert_eq!(
        report.files[0].findings[0].location.path.as_str(),
        "second.js"
    );
}

#[test]
fn session_retry_does_not_cache_parse_failure() {
    let linter = test_linter();
    let mut session = linter.begin_project("/project").unwrap();
    session
        .admit_test_source(source_file("broken.js", "fetch("))
        .unwrap();
    let observer = CountingExecutionObserver::new();
    session
        .analyze_source_counted("broken.js", &observer)
        .unwrap();
    session
        .analyze_source_counted("broken.js", &observer)
        .unwrap();
    assert_eq!(observer.invocations().parses, 2);
    assert_eq!(observer.invocations().lowers, 2);
    assert_eq!(observer.invocations().misses, 2);
    assert_eq!(observer.invocations().inserts, 0);
    let report = finish_collection(session);
    assert_eq!(report.files[0].diagnostics.len(), 1);
    assert!(matches!(
        report.files[0].diagnostics[0],
        Diagnostic::Parse { .. }
    ));
}

#[test]
fn session_reuses_exhausted_artifact_with_partial_status() {
    let limits = AnalysisLimits {
        semantic_operations: 1,
        ..AnalysisLimits::default()
    };
    let linter = test_linter_with_limits(limits);
    let mut session = linter.begin_project("/project").unwrap();
    session
        .admit_test_source(source_file("bounded.js", "fetch('/api');"))
        .unwrap();
    let observer = CountingExecutionObserver::new();
    session
        .analyze_source_counted("bounded.js", &observer)
        .unwrap();
    session
        .analyze_source_counted("bounded.js", &observer)
        .unwrap();
    assert_eq!(observer.invocations().lowers, 1);
    assert_eq!(observer.invocations().hits, 1);
    let report = finish_collection(session);
    assert_eq!(report.completion, ReportCompletion::Partial);
    assert!(
        report.files[0]
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code() == "semantic_budget_exhausted")
    );
}

#[test]
fn rule_selection_changes_projection_without_relowering() {
    let enabled = test_linter();
    let mut first = enabled.begin_project("/project").unwrap();
    first
        .admit_test_source(source_file("enabled.js", "fetch('/api');"))
        .unwrap();
    let first_observer = CountingExecutionObserver::new();
    first
        .analyze_source_counted("enabled.js", &first_observer)
        .unwrap();
    let cache = std::mem::take(&mut first.artifact_cache);
    assert_eq!(finish_collection(first).files[0].findings.len(), 1);

    let disabled = test_linter_with_selection(
        RuleSelection::new(RuleBaseline::None),
        AnalysisLimits::default(),
    );
    let mut second = disabled.begin_project("/project").unwrap();
    second.artifact_cache = cache;
    second
        .admit_test_source(source_file("disabled.js", "fetch('/api');"))
        .unwrap();
    let second_observer = CountingExecutionObserver::new();
    second
        .analyze_source_counted("disabled.js", &second_observer)
        .unwrap();
    assert_eq!(second_observer.invocations().hits, 1);
    assert_eq!(second_observer.invocations().lowers, 0);
    assert!(finish_collection(second).files[0].findings.is_empty());
}

#[test]
fn all_fingerprint_dimensions_have_independent_hit_miss_tests() {
    let base_linter = test_linter();
    let mut baseline = base_linter.begin_project("/project").unwrap();
    baseline
        .admit_test_source(source_file("base.js", "fetch('/api');"))
        .unwrap();
    baseline
        .analyze_source_at_path(&crate::ProjectRelativePath::new("base.js").unwrap())
        .unwrap();
    let base_cache = baseline.artifact_cache.clone();

    let assert_miss = |linter: &crate::Linter,
                       source: SourceFile,
                       configure: fn(&mut crate::ProjectCollection<'_>)| {
        let path = source.path.to_string();
        let mut session = linter.begin_project("/project").unwrap();
        session.artifact_cache = base_cache.clone();
        configure(&mut session);
        session.admit_test_source(source).unwrap();
        let observer = CountingExecutionObserver::new();
        session.analyze_source_counted(path, &observer).unwrap();
        let counts = observer.invocations();
        assert_eq!((counts.hits, counts.misses, counts.lowers), (0, 1, 1));
    };

    assert_miss(
        &base_linter,
        source_file("changed.js", "fetch('/different');"),
        |_| {},
    );
    assert_miss(
        &base_linter,
        SourceFile {
            path: project_path("typed.ts"),
            language: SourceLanguage::TypeScript,
            source: "fetch('/api');".into(),
        },
        |_| {},
    );

    let mut changed_environment = Environment::default();
    changed_environment.add_globals(["fetch", "extra"]).unwrap();
    let environment_linter = test_linter_with_environment(changed_environment);
    assert_miss(
        &environment_linter,
        source_file("environment.js", "fetch('/api');"),
        |_| {},
    );

    let defaults = AnalysisLimits::default();
    let changed_limits = [
        AnalysisLimits {
            syntax_depth: defaults.syntax_depth + 1,
            ..defaults.clone()
        },
        AnalysisLimits {
            semantic_operations: defaults.semantic_operations + 1,
            ..defaults.clone()
        },
        AnalysisLimits {
            effect_operations: defaults.effect_operations + 1,
            ..defaults.clone()
        },
        AnalysisLimits {
            evidence_items: defaults.evidence_items + 1,
            ..defaults.clone()
        },
        AnalysisLimits {
            link_operations: defaults.link_operations + 1,
            ..defaults.clone()
        },
        AnalysisLimits {
            flow_operations: defaults.flow_operations + 1,
            ..defaults
        },
    ];
    for (index, limits) in changed_limits.into_iter().enumerate() {
        let linter = test_linter_with_limits(limits);
        assert_miss(
            &linter,
            source_file(format!("limit-{index}.js"), "fetch('/api');"),
            |_| {},
        );
    }

    assert_miss(
        &base_linter,
        source_file("normalization.js", "fetch('/api');"),
        |session| session.set_fingerprint_normalization("test-normalization-v2"),
    );
    assert_miss(
        &base_linter,
        source_file("engine.js", "fetch('/api');"),
        |session| session.set_fingerprint_engine_version("test-engine-v2"),
    );
}

#[test]
fn cache_eviction_is_bounded_and_deterministic() {
    let linter = test_linter();
    let mut session = linter.begin_project("/project").unwrap();
    let capacity = ArtifactCacheHandle::capacity();
    for index in 0..=capacity {
        session
            .admit_test_source(source_file(
                format!("{index:03}.js"),
                format!("fetch('/{index}');"),
            ))
            .unwrap();
    }
    let observer = CountingExecutionObserver::new();
    for index in 0..=capacity {
        session
            .analyze_source_counted(format!("{index:03}.js"), &observer)
            .unwrap();
    }
    assert_eq!(observer.invocations().inserts, capacity + 1);
    assert_eq!(observer.invocations().evictions, 1);
    session.analyze_source_counted("000.js", &observer).unwrap();
    assert_eq!(observer.invocations().misses, capacity + 2);
    assert_eq!(observer.invocations().evictions, 2);
    session
        .analyze_source_counted(format!("{capacity:03}.js"), &observer)
        .unwrap();
    assert_eq!(observer.invocations().hits, 1);
}
