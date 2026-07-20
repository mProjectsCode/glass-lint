//! Project-session integration tests for normalization, linking, flow, and
//! report ownership across multiple authored source files.

use crate::{
    Position, SourceRange,
    api::rule::{
        CallMatcher, Confidence, FlowCompletion, FlowCondition, FlowSinkMatcher, Matcher,
        MemberCallMatcher, ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, Rule,
        Severity, ValueMatcher,
    },
    project::{
        session::{
            ControlledReleaseOrder, CountingExecutionObserver, InvocationCounts,
            outstanding_job_bound,
        },
        *,
    },
};

fn source_file(path: impl Into<String>, source: impl Into<String>) -> SourceFile {
    SourceFile::new(path, source).unwrap()
}

fn project_path(path: &str) -> ProjectRelativePath {
    ProjectRelativePath::new(path).unwrap()
}

#[test]
fn admitted_sources_have_identical_reports_across_worker_counts() {
    let sources = [
        source_file("z.js", "fetch('/z');"),
        source_file("a.js", "fetch('/a');"),
        source_file("m.js", "function f(x) { return x; } f(fetch('/m'));"),
    ];
    let mut reports = Vec::new();
    for workers in [0, 1, 2, 4] {
        let linter = test_linter();
        let mut session = linter.begin_analysis("/project").unwrap();
        for source in sources.iter().cloned() {
            session.admit_source(source).unwrap();
        }
        session.analyze_admitted_sources(workers).unwrap();
        let report = session.finish().unwrap();
        reports.push(serde_json::to_value(report).unwrap());
    }
    assert!(reports.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn controlled_release_orders_produce_identical_full_report() {
    let limits = crate::AnalysisLimits {
        semantic_operations: 40,
        ..crate::AnalysisLimits::default()
    };
    let sources = [
        source_file("z.js", "import value from './dependency.js'; fetch('/z');"),
        source_file("broken.js", "fetch("),
        source_file("a.js", "fetch('/a');"),
        source_file("partial.js", "fetch('/partial');".repeat(100)),
    ];
    let mut reports = Vec::new();
    for order in [
        ControlledReleaseOrder::Forward,
        ControlledReleaseOrder::Reverse,
        ControlledReleaseOrder::Interleaved,
    ] {
        let linter = test_linter_with_limits(limits.clone());
        let mut session = linter.begin_analysis("/project").unwrap();
        for source in sources.iter().cloned() {
            session.admit_source(source).unwrap();
        }
        session
            .analyze_admitted_sources_controlled(2, order)
            .unwrap();
        reports.push(serde_json::to_value(session.finish().unwrap()).unwrap());
    }
    assert!(reports.windows(2).all(|pair| pair[0] == pair[1]));
    let report = &reports[0];
    assert!(report["files"].as_array().unwrap().iter().any(|file| {
        file["findings"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    }));
    let diagnostics = report["files"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|file| file["diagnostics"].as_array().unwrap())
        .collect::<Vec<_>>();
    assert!(diagnostics.iter().any(|item| item["kind"] == "parse"));
    assert!(
        diagnostics
            .iter()
            .any(|item| item["code"] == "semantic_budget_exhausted")
    );
    assert!(
        diagnostics
            .iter()
            .any(|item| item["code"] == "unresolved_internal_request")
    );
}

#[test]
fn active_and_outstanding_use_the_production_bound() {
    for requested in [0, 1, 2, 4] {
        let linter = test_linter();
        let mut session = linter.begin_analysis("/project").unwrap();
        for index in 0..12 {
            session
                .admit_source(source_file(format!("{index:02}.js"), "fetch('x');"))
                .unwrap();
        }
        let observer = CountingExecutionObserver::new();
        session
            .analyze_admitted_sources_counted(requested, &observer)
            .unwrap();
        let workers = std::num::NonZeroUsize::new(requested).unwrap_or(std::num::NonZeroUsize::MIN);
        let (active, outstanding) = observer.peaks();
        assert!(active <= workers.get());
        assert!(outstanding <= outstanding_job_bound(workers));
    }
    assert_eq!(
        outstanding_job_bound(std::num::NonZeroUsize::MAX),
        usize::MAX
    );
}

#[test]
fn cache_hit_attaches_only_current_path() {
    let linter = test_linter();
    let mut session = linter.begin_analysis("/project").unwrap();
    let observer = CountingExecutionObserver::new();
    session
        .admit_source(source_file("a.js", "fetch('x');"))
        .unwrap();
    session.analyze_source_counted("a.js", &observer).unwrap();
    session
        .admit_source(source_file("b.js", "fetch('x');"))
        .unwrap();
    session.analyze_source_counted("b.js", &observer).unwrap();
    assert_eq!(observer.invocations().hits, 1);
    let report = session.finish().unwrap();
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
    let mut session = linter.begin_analysis("/project").unwrap();
    session
        .admit_source(source_file("main.js", "fetch('/api');"))
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
    assert_eq!(session.finish().unwrap().files[0].findings.len(), 1);
}

#[test]
fn separate_sessions_on_one_linter_reuse_the_artifact_cache() {
    let linter = test_linter();
    let first_observer = CountingExecutionObserver::new();
    let mut first = linter.begin_analysis("/project").unwrap();
    first
        .admit_source(source_file("first.js", "fetch('/api');"))
        .unwrap();
    first
        .analyze_source_counted("first.js", &first_observer)
        .unwrap();
    first.finish().unwrap();

    let second_observer = CountingExecutionObserver::new();
    let mut second = linter.begin_analysis("/project").unwrap();
    second
        .admit_source(source_file("second.js", "fetch('/api');"))
        .unwrap();
    second
        .analyze_source_counted("second.js", &second_observer)
        .unwrap();
    let report = second.finish().unwrap();

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
    let mut session = linter.begin_analysis("/project").unwrap();
    session
        .admit_source(source_file("broken.js", "fetch("))
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
    let report = session.finish().unwrap();
    assert_eq!(report.files[0].diagnostics.len(), 1);
    assert!(matches!(
        report.files[0].diagnostics[0],
        Diagnostic::Parse { .. }
    ));
}

#[test]
fn session_reuses_exhausted_artifact_with_partial_status() {
    let limits = crate::AnalysisLimits {
        semantic_operations: 1,
        ..crate::AnalysisLimits::default()
    };
    let linter = test_linter_with_limits(limits);
    let mut session = linter.begin_analysis("/project").unwrap();
    session
        .admit_source(source_file("bounded.js", "fetch('/api');"))
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
    let report = session.finish().unwrap();
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
    let mut first = enabled.begin_analysis("/project").unwrap();
    first
        .admit_source(source_file("enabled.js", "fetch('/api');"))
        .unwrap();
    let first_observer = CountingExecutionObserver::new();
    first
        .analyze_source_counted("enabled.js", &first_observer)
        .unwrap();
    let cache = std::mem::take(&mut first.artifact_cache);
    assert_eq!(first.finish().unwrap().files[0].findings.len(), 1);

    let disabled = test_linter_with_selection(
        crate::RuleSelection::new(crate::RuleBaseline::None),
        crate::AnalysisLimits::default(),
    );
    let mut second = disabled.begin_analysis("/project").unwrap();
    second.artifact_cache = cache;
    second
        .admit_source(source_file("disabled.js", "fetch('/api');"))
        .unwrap();
    let second_observer = CountingExecutionObserver::new();
    second
        .analyze_source_counted("disabled.js", &second_observer)
        .unwrap();
    assert_eq!(second_observer.invocations().hits, 1);
    assert_eq!(second_observer.invocations().lowers, 0);
    assert!(second.finish().unwrap().files[0].findings.is_empty());
}

#[test]
fn all_fingerprint_dimensions_have_independent_hit_miss_tests() {
    let base_linter = test_linter();
    let mut baseline = base_linter.begin_analysis("/project").unwrap();
    baseline
        .admit_source(source_file("base.js", "fetch('/api');"))
        .unwrap();
    baseline.analyze_source("base.js").unwrap();
    let base_cache = baseline.artifact_cache.clone();

    let assert_miss =
        |linter: &crate::Linter, source: SourceFile, configure: fn(&mut AnalysisSession<'_>)| {
            let path = source.path.to_string();
            let mut session = linter.begin_analysis("/project").unwrap();
            session.artifact_cache = base_cache.clone();
            configure(&mut session);
            session.admit_source(source).unwrap();
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
            language: crate::SourceLanguage::TypeScript,
            source: "fetch('/api');".into(),
        },
        |_| {},
    );

    let mut changed_environment = crate::Environment::default();
    changed_environment.add_globals(["fetch", "extra"]).unwrap();
    let environment_linter = test_linter_with_environment(changed_environment);
    assert_miss(
        &environment_linter,
        source_file("environment.js", "fetch('/api');"),
        |_| {},
    );

    let defaults = crate::AnalysisLimits::default();
    let changed_limits = [
        crate::AnalysisLimits {
            syntax_depth: defaults.syntax_depth + 1,
            ..defaults.clone()
        },
        crate::AnalysisLimits {
            semantic_operations: defaults.semantic_operations + 1,
            ..defaults.clone()
        },
        crate::AnalysisLimits {
            effect_operations: defaults.effect_operations + 1,
            ..defaults.clone()
        },
        crate::AnalysisLimits {
            evidence_items: defaults.evidence_items + 1,
            ..defaults.clone()
        },
        crate::AnalysisLimits {
            link_operations: defaults.link_operations + 1,
            ..defaults.clone()
        },
        crate::AnalysisLimits {
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
    let mut session = linter.begin_analysis("/project").unwrap();
    let capacity = crate::analysis::ArtifactCacheHandle::capacity();
    for index in 0..=capacity {
        session
            .admit_source(source_file(
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

#[test]
fn project_relative_paths_validate_construction_and_deserialization() {
    for invalid in [
        "",
        "/absolute.js",
        "../escape.js",
        "dir/../escape.js",
        "\0.js",
    ] {
        assert!(ProjectRelativePath::new(invalid).is_err(), "{invalid:?}");
        let json = serde_json::to_string(invalid).unwrap();
        assert!(serde_json::from_str::<ProjectRelativePath>(&json).is_err());
    }

    let normalized = ProjectRelativePath::new("src\\unicode/./é.js").unwrap();
    assert_eq!(normalized.as_str(), "src/unicode/é.js");
    let json = serde_json::to_string(&normalized).unwrap();
    assert_eq!(json, "\"src/unicode/é.js\"");
}

fn test_linter() -> crate::Linter {
    let mut environment = crate::Environment::default();
    environment.add_global("fetch").unwrap();
    test_linter_with_environment(environment)
}

fn test_linter_with_environment(environment: crate::Environment) -> crate::Linter {
    let rule = Rule::builder("network.fetch")
        .description("Uses fetch")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap()
}

fn test_linter_with_limits(limits: crate::AnalysisLimits) -> crate::Linter {
    let mut environment = crate::Environment::default();
    environment.add_global("fetch").unwrap();
    let rule = Rule::builder("network.fetch")
        .description("Uses fetch")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    crate::Linter::new(
        crate::LinterConfig::new(
            vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
            environment,
        )
        .with_limits(limits),
    )
    .unwrap()
}

fn test_linter_with_selection(
    selection: crate::RuleSelection,
    limits: crate::AnalysisLimits,
) -> crate::Linter {
    let mut environment = crate::Environment::default();
    environment.add_global("fetch").unwrap();
    let rule = Rule::builder("network.fetch")
        .description("Uses fetch")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    crate::Linter::new(
        crate::LinterConfig::new(
            vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
            environment,
        )
        .with_rules(selection)
        .with_limits(limits),
    )
    .unwrap()
}

fn flow_linter() -> crate::Linter {
    let rule = Rule::builder("flow.append")
        .description("Appends a configured script")
        .category("flow")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(
            ObjectFlowMatcher::builder("script insertion")
                .source(ObjectSourceMatcher::returned_by(
                    MemberCallMatcher::rooted("document.createElement")
                        .arg(0, ValueMatcher::static_string().equals("script")),
                ))
                .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
                    "src",
                    ValueMatcher::any_value(),
                )))
                .complete_at(FlowCompletion::any_sink([FlowSinkMatcher::argument_of(
                    MemberCallMatcher::rooted("document.head.appendChild"),
                    0,
                )]))
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();
    let mut environment = crate::Environment::default();
    environment
        .add_globals(["document", "url"])
        .expect("test environment globals");
    crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap()
}

fn key(importer: &str) -> ResolutionRequestKey {
    ResolutionRequestKey {
        importer: ProjectRelativePath::new(importer).unwrap(),
        kind: ResolutionRequestKind::StaticImport,
        range: SourceRange::new(Position::new(1, 1).unwrap(), Position::new(1, 8).unwrap())
            .unwrap(),
    }
}

struct ProjectFixture<'a> {
    session: AnalysisSession<'a>,
}

impl<'a> ProjectFixture<'a> {
    fn new(linter: &'a crate::Linter) -> Self {
        Self {
            session: linter.begin_analysis("/project").unwrap(),
        }
    }

    fn add(&mut self, path: &str, source: &str) {
        self.session.add_source(source_file(path, source)).unwrap();
    }

    fn add_resolved(
        &mut self,
        path: &str,
        source: &str,
        resolutions: impl IntoIterator<Item = ResolverOutcome>,
    ) {
        let requests = self.session.add_source(source_file(path, source)).unwrap();
        for (request, resolution) in requests.into_iter().zip(resolutions) {
            self.session
                .record_resolution(request.key, resolution)
                .unwrap();
        }
    }

    fn finish(self) -> AnalysisReport {
        self.session.finish().unwrap()
    }
}

#[path = "tests/input_validation.rs"]
mod input_validation;

#[test]
fn session_uses_project_analysis_and_preserves_single_file_findings() {
    let linter = test_linter();
    let source = "fetch('/remote');\n";
    let direct = linter.lint_snippet(source, "a.js").unwrap();

    let mut session = linter.begin_analysis("/project").unwrap();
    session.add_source(source_file("a.js", source)).unwrap();
    let project = session.finish().unwrap();

    assert_eq!(project.files.len(), 1);
    assert_eq!(project.files[0].path, "a.js");
    assert_eq!(
        project.files[0].findings.len(),
        direct.files[0].findings.len()
    );
    assert_eq!(
        project.files[0].findings[0].location.range,
        direct.files[0].findings[0].location.range
    );
    assert_eq!(project.files[0].findings[0].location.path, "a.js");
    assert_eq!(
        project.files[0].findings[0].evidence[0]
            .location
            .as_ref()
            .map(|location| location.path.as_str()),
        Some("a.js")
    );
}

#[path = "tests/linking_and_flow.rs"]
mod linking_and_flow;
#[path = "tests/session_and_link_validation.rs"]
mod session_and_link_validation;
#[path = "tests/status_policy.rs"]
mod status_policy;
