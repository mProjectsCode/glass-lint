//! Project-session integration tests for normalization, linking, flow, and
//! report ownership across multiple authored source files.

use super::*;
use crate::{
    Position, SourceRange,
    api::rule::{
        CallMatcher, Confidence, FlowCompletion, FlowCondition, FlowSinkMatcher, Matcher,
        MemberCallMatcher, ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, Rule,
        Severity, ValueMatcher,
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
        super::session::ControlledReleaseOrder::Forward,
        super::session::ControlledReleaseOrder::Reverse,
        super::session::ControlledReleaseOrder::Interleaved,
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
        let observer = super::session::CountingExecutionObserver::new();
        session
            .analyze_admitted_sources_counted(requested, &observer)
            .unwrap();
        let workers = std::num::NonZeroUsize::new(requested).unwrap_or(std::num::NonZeroUsize::MIN);
        let (active, outstanding) = observer.peaks();
        assert!(active <= workers.get());
        assert!(outstanding <= super::session::outstanding_job_bound(workers));
    }
    assert_eq!(
        super::session::outstanding_job_bound(std::num::NonZeroUsize::MAX),
        usize::MAX
    );
}

#[test]
fn cache_hit_attaches_only_current_path() {
    let linter = test_linter();
    let mut session = linter.begin_analysis("/project").unwrap();
    let observer = super::session::CountingExecutionObserver::new();
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
    let observer = super::session::CountingExecutionObserver::new();
    session
        .analyze_source_counted("main.js", &observer)
        .unwrap();
    session
        .analyze_source_counted("main.js", &observer)
        .unwrap();
    assert_eq!(
        observer.invocations(),
        super::session::InvocationCounts {
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
    let first_observer = super::session::CountingExecutionObserver::new();
    let mut first = linter.begin_analysis("/project").unwrap();
    first
        .admit_source(source_file("first.js", "fetch('/api');"))
        .unwrap();
    first
        .analyze_source_counted("first.js", &first_observer)
        .unwrap();
    first.finish().unwrap();

    let second_observer = super::session::CountingExecutionObserver::new();
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
    let observer = super::session::CountingExecutionObserver::new();
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
    let observer = super::session::CountingExecutionObserver::new();
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
    let first_observer = super::session::CountingExecutionObserver::new();
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
    let second_observer = super::session::CountingExecutionObserver::new();
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
            let observer = super::session::CountingExecutionObserver::new();
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
    let observer = super::session::CountingExecutionObserver::new();
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
        .label("Uses fetch")
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
        .label("Uses fetch")
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
        .label("Uses fetch")
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
        .label("Appends a configured script")
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
        kind: ResolutionRequestKind::Import,
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
        resolutions: impl IntoIterator<Item = ResolutionResult>,
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

mod input_validation {
    use super::*;

    #[test]
    fn validation_normalizes_and_sorts_sources_and_edges() {
        let input = ProjectInput {
            root: "/project".into(),
            sources: vec![source_file("./z.js", ""), source_file("a.js", "")],
            resolutions: vec![(
                key("./z.js"),
                ResolutionResult::Internal {
                    path: project_path("./a.js"),
                },
            )],
        }
        .validate()
        .unwrap();

        assert_eq!(
            input
                .sources
                .iter()
                .map(|source| source.path.as_str())
                .collect::<Vec<_>>(),
            ["a.js", "z.js"]
        );
        assert_eq!(input.resolutions[0].0.importer, "z.js");
        assert_eq!(
            input.resolutions[0].1,
            ResolutionResult::Internal {
                path: project_path("a.js")
            }
        );
        assert_eq!(input.module_ids()["a.js"], ModuleId::new(0));
        assert_eq!(input.module_ids()["z.js"], ModuleId::new(1));
    }

    #[test]
    fn duplicate_and_foreign_records_are_rejected() {
        let duplicate = ProjectInput {
            root: "/project".into(),
            sources: vec![source_file("a.js", ""), source_file("./a.js", "")],
            resolutions: vec![],
        }
        .validate();
        assert!(matches!(
            duplicate,
            Err(ProjectInputError::DuplicateSource(_))
        ));

        let foreign = ProjectInput {
            root: "/project".into(),
            sources: vec![source_file("a.js", "")],
            resolutions: vec![(key("missing.js"), ResolutionResult::Missing)],
        }
        .validate();
        assert!(matches!(
            foreign,
            Err(ProjectInputError::UnknownImporter(_))
        ));
    }
}

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

mod linking_and_flow {
    use super::*;

    #[test]
    fn linked_internal_aliases_preserve_external_and_global_call_identity() {
        let external_rule = Rule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let global_rule = Rule::builder("network.fetch")
            .label("Uses fetch")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment.add_global("fetch").unwrap();
        let linter = crate::Linter::new(crate::LinterConfig::new(
            vec![crate::RuleCatalog::new("test", vec![external_rule, global_rule]).unwrap()],
            environment,
        ))
        .unwrap();

        let mut session = linter.begin_analysis("/project").unwrap();
        let helper = session
            .add_source(source_file(
                "helper.js",
                "import { request } from 'web'; export { request as send };",
            ))
            .unwrap();
        let main = session
            .add_source(source_file(
                "main.js",
                "import { send } from './helper'; send();",
            ))
            .unwrap();
        session
            .record_resolution(
                helper[0].key.clone(),
                ResolutionResult::External {
                    package: "web".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: project_path("helper.js"),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();
        let main_report = report
            .files
            .iter()
            .find(|file| file.path == "main.js")
            .unwrap();
        assert_eq!(main_report.findings.len(), 1);
        assert_eq!(
            main_report.findings[0].rule_id.as_str(),
            "test:network.request"
        );

        let mut global = linter.begin_analysis("/project").unwrap();
        let helper = global
            .add_source(source_file("helper.js", "export { fetch as send };"))
            .unwrap();
        let main = global
            .add_source(source_file(
                "main.js",
                "import { send } from './helper'; send();",
            ))
            .unwrap();
        assert!(helper.is_empty());
        global
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: project_path("helper.js"),
                },
            )
            .unwrap();
        let report = global.finish().unwrap();
        let main_report = report
            .files
            .iter()
            .find(|file| file.path == "main.js")
            .unwrap();
        assert_eq!(main_report.findings.len(), 1);
        assert_eq!(
            main_report.findings[0].rule_id.as_str(),
            "test:network.fetch"
        );
    }

    #[test]
    fn project_flow_crosses_an_exported_helper_boundary() {
        let linter = flow_linter();
        let mut project = ProjectFixture::new(&linter);
        project.add(
        "helper.js",
        "export function append(element) { element.src = url; document.head.appendChild(element); }",
    );
        project.add_resolved(
        "main.js",
        "import { append } from './helper'; const element = document.createElement('script'); append(element);",
        [ResolutionResult::Internal {
            path: project_path("helper.js"),
        }],
    );
        let report = project.finish();
        let main = report
            .files
            .iter()
            .find(|file| file.path == "helper.js")
            .expect("helper report");
        assert_eq!(main.findings.len(), 1);
        assert_eq!(main.findings[0].location.path, "helper.js");
    }

    #[test]
    fn project_flow_preserves_requirements_through_a_helper_chain() {
        let linter = flow_linter();
        let mut project = ProjectFixture::new(&linter);
        project.add(
            "sink.js",
            "export function finish(element) { document.head.appendChild(element); }",
        );
        project.add_resolved(
        "helper.js",
        "import { finish } from './sink'; export function append(element) { element.src = url; finish(element); }",
        [ResolutionResult::Internal {
            path: project_path("sink.js"),
        }],
    );
        project.add_resolved(
        "main.js",
        "import { append } from './helper'; const element = document.createElement('script'); append(element);",
        [ResolutionResult::Internal {
            path: project_path("helper.js"),
        }],
    );
        let report = project.finish();
        assert!(report.files.iter().any(|file| !file.findings.is_empty()));
    }

    #[test]
    fn project_flow_follows_a_returned_parameter() {
        let linter = flow_linter();
        let mut project = ProjectFixture::new(&linter);
        project.add(
            "helper.js",
            "export function identity(element) { return element; }",
        );
        project.add_resolved(
        "main.js",
        "import { identity } from './helper'; const element = document.createElement('script'); const returned = identity(element); returned.src = url; document.head.appendChild(returned);",
        [ResolutionResult::Internal {
            path: project_path("helper.js"),
        }],
    );
        let report = project.finish();
        let main = report
            .files
            .iter()
            .find(|file| file.path == "main.js")
            .unwrap();
        assert_eq!(main.findings.len(), 1);
    }

    #[test]
    fn project_flow_fails_closed_for_unsupported_helper_control_flow() {
        let linter = flow_linter();
        let mut project = ProjectFixture::new(&linter);
        project.add(
        "helper.js",
        "export function append(element) { if (ready) element.src = url; document.head.appendChild(element); }",
    );
        project.add_resolved(
        "main.js",
        "import { append } from './helper'; const element = document.createElement('script'); append(element);",
        [ResolutionResult::Internal {
            path: project_path("helper.js"),
        }],
    );
        let report = project.finish();
        assert!(report.files.iter().all(|file| file.findings.is_empty()));
    }

    #[test]
    fn linked_unknown_exports_and_importer_reassignment_fail_closed() {
        let rule = Rule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(crate::LinterConfig::new(
            vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
            crate::Environment::default(),
        ))
        .unwrap();

        let mut session = linter.begin_analysis("/project").unwrap();
        let helper = session
            .add_source(source_file(
                "helper.js",
                "import { request } from 'web'; export { request as send };",
            ))
            .unwrap();
        let main = session
            .add_source(source_file(
                "main.js",
                "import { send } from './helper'; send = local; send();",
            ))
            .unwrap();
        session
            .record_resolution(
                helper[0].key.clone(),
                ResolutionResult::External {
                    package: "web".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: project_path("helper.js"),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();
        assert_eq!(
            report
                .files
                .iter()
                .find(|file| file.path == "main.js")
                .unwrap()
                .findings
                .len(),
            0
        );

        let mut missing = linter.begin_analysis("/project").unwrap();
        let main = missing
            .add_source(source_file(
                "main.js",
                "import { send } from './helper'; send();",
            ))
            .unwrap();
        missing
            .add_source(source_file("helper.js", "export const other = 1;"))
            .unwrap();
        missing
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: project_path("helper.js"),
                },
            )
            .unwrap();
        let report = missing.finish().unwrap();
        assert_eq!(
            report
                .files
                .iter()
                .find(|file| file.path == "main.js")
                .unwrap()
                .findings
                .len(),
            0
        );
    }

    #[test]
    fn unresolved_internal_imports_do_not_become_external_provenance() {
        let rule = Rule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("./helper", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(crate::LinterConfig::new(
            vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
            crate::Environment::default(),
        ))
        .unwrap();

        let mut session = linter.begin_analysis("/project").unwrap();
        session
            .add_source(source_file(
                "main.js",
                "import { request } from './helper'; request();",
            ))
            .unwrap();
        let report = session.finish().unwrap();

        assert!(report.files.iter().all(|file| file.findings.is_empty()));
        assert!(
            report
                .files
                .iter()
                .flat_map(|file| &file.diagnostics)
                .any(|diagnostic| diagnostic.code() == "unresolved_internal_request")
        );
    }

    #[test]
    fn commonjs_export_aliases_preserve_external_provenance_across_modules() {
        let rule = Rule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(crate::LinterConfig::new(
            vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
            crate::Environment::default(),
        ))
        .unwrap();

        let mut project = ProjectFixture::new(&linter);
        project.add_resolved(
            "helper.js",
            "const { request } = require('web'); exports.send = request;",
            [ResolutionResult::External {
                package: "web".into(),
            }],
        );
        project.add_resolved(
            "main.js",
            "const { send } = require('./helper'); send();",
            [ResolutionResult::Internal {
                path: project_path("helper.js"),
            }],
        );
        let report = project.finish();

        let main_report = report
            .files
            .iter()
            .find(|file| file.path == "main.js")
            .unwrap();
        assert_eq!(main_report.findings.len(), 1);
    }

    #[test]
    fn namespace_imports_follow_star_reexports() {
        let rule = Rule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(crate::LinterConfig::new(
            vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
            crate::Environment::default(),
        ))
        .unwrap();

        let mut project = ProjectFixture::new(&linter);
        project.add_resolved(
            "helper.js",
            "import { request } from 'web'; export { request };",
            [ResolutionResult::External {
                package: "web".into(),
            }],
        );
        project.add_resolved(
            "barrel.js",
            "export * from './helper';",
            [ResolutionResult::Internal {
                path: project_path("helper.js"),
            }],
        );
        project.add_resolved(
            "main.js",
            "import * as api from './barrel'; api.request();",
            [ResolutionResult::Internal {
                path: project_path("barrel.js"),
            }],
        );
        let report = project.finish();

        let main_report = report
            .files
            .iter()
            .find(|file| file.path == "main.js")
            .unwrap();
        assert_eq!(main_report.findings.len(), 1);
    }

    #[test]
    fn static_dynamic_imports_follow_namespace_exports() {
        let rule = Rule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(crate::LinterConfig::new(
            vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
            crate::Environment::default(),
        ))
        .unwrap();
        let mut project = ProjectFixture::new(&linter);
        project.add_resolved(
            "helper.js",
            "import { request } from 'web'; export { request };",
            [ResolutionResult::External {
                package: "web".into(),
            }],
        );
        project.add_resolved(
            "main.js",
            "async function run() { const api = await import('./helper'); api.request(); }",
            [ResolutionResult::Internal {
                path: project_path("helper.js"),
            }],
        );
        let report = project.finish();
        assert_eq!(
            report
                .files
                .iter()
                .find(|file| file.path == "main.js")
                .unwrap()
                .findings
                .len(),
            1
        );
    }

    #[test]
    fn anonymous_commonjs_functions_remain_callable_across_modules() {
        let rule = Rule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(crate::LinterConfig::new(
            vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
            crate::Environment::default(),
        ))
        .unwrap();
        let mut project = ProjectFixture::new(&linter);
        project.add_resolved(
            "helper.js",
            "const { request } = require('web'); exports.send = () => request();",
            [ResolutionResult::External {
                package: "web".into(),
            }],
        );
        project.add_resolved(
            "main.js",
            "const { send } = require('./helper'); send();",
            [ResolutionResult::Internal {
                path: project_path("helper.js"),
            }],
        );
        let report = project.finish();
        assert_eq!(
            report
                .files
                .iter()
                .find(|file| file.path == "helper.js")
                .unwrap()
                .findings
                .len(),
            1
        );
    }

    #[test]
    fn returned_callable_provenance_crosses_an_exported_function() {
        let rule = Rule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::call(
                CallMatcher::module_export("web", "request").static_string_arg(0),
            ))
            .build()
            .unwrap();
        let linter = crate::Linter::new(crate::LinterConfig::new(
            vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
            crate::Environment::default(),
        ))
        .unwrap();
        let mut project = ProjectFixture::new(&linter);
        project.add_resolved(
            "helper.js",
            "import { request } from 'web'; export function get() { return request; }",
            [ResolutionResult::External {
                package: "web".into(),
            }],
        );
        project.add_resolved(
            "main.js",
            "import { get } from './helper'; get()('/remote');",
            [ResolutionResult::Internal {
                path: project_path("helper.js"),
            }],
        );
        let report = project.finish();
        assert_eq!(
            report
                .files
                .iter()
                .find(|file| file.path == "main.js")
                .unwrap()
                .findings
                .len(),
            1
        );
    }

    #[test]
    fn linked_external_call_arguments_are_projected_after_reexports() {
        let rule = Rule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::call(
                CallMatcher::module_export("web", "request").static_string_arg(0),
            ))
            .build()
            .unwrap();
        let linter = crate::Linter::new(crate::LinterConfig::new(
            vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
            crate::Environment::default(),
        ))
        .unwrap();

        let mut project = ProjectFixture::new(&linter);
        project.add_resolved(
            "helper.js",
            "import { request } from 'web'; export { request as send };",
            [ResolutionResult::External {
                package: "web".into(),
            }],
        );
        project.add_resolved(
            "main.js",
            "import { send } from './helper'; send('/remote');",
            [ResolutionResult::Internal {
                path: project_path("helper.js"),
            }],
        );
        let report = project.finish();

        assert_eq!(
            report
                .files
                .iter()
                .find(|file| file.path == "main.js")
                .unwrap()
                .findings
                .len(),
            1
        );
    }
}

#[test]
fn project_keeps_sorted_parse_failures_separate_from_valid_modules() {
    let linter = test_linter();
    let mut session = linter.begin_analysis("/project").unwrap();
    session
        .add_source(source_file("z.js", "function {"))
        .unwrap();
    session
        .add_source(source_file("a.js", "fetch('/remote');"))
        .unwrap();

    let report = session.finish().unwrap();
    assert_eq!(
        report
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["a.js", "z.js"]
    );
    assert_eq!(report.files[0].findings.len(), 1);
    assert_eq!(report.files[1].findings.len(), 0);
    assert_eq!(report.files[1].parse_diagnostic_count(), 1);
}

#[test]
fn session_returns_static_import_dynamic_import_require_and_reexport_requests() {
    let linter = test_linter();
    let mut session = linter.begin_analysis("/project").unwrap();
    let requests = session
            .add_source(source_file(
                "main.js",
                "import { value as local } from './dep';\nexport { local as renamed } from './dep';\nconst x = require('./cjs');\nimport('./lazy');",
            ))
            .unwrap();
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests
            .iter()
            .map(|request| request.key.kind)
            .collect::<Vec<_>>(),
        vec![
            ResolutionRequestKind::Import,
            ResolutionRequestKind::Import,
            ResolutionRequestKind::Require,
            ResolutionRequestKind::DynamicImport,
        ]
    );
    assert_eq!(requests[0].request, "./dep");
    assert_eq!(requests[2].request, "./cjs");
    assert_eq!(requests[3].request, "./lazy");
    assert_eq!(requests[2].key.range.start().column(), 19);
    assert_eq!(requests[2].key.range.end().column(), 26);
}

#[test]
fn session_rejects_resolution_for_an_unauthored_request() {
    let linter = test_linter();
    let mut session = linter.begin_analysis("/project").unwrap();
    session
        .add_source(source_file("main.js", "fetch('/remote');"))
        .unwrap();
    let error = session.record_resolution(key("main.js"), ResolutionResult::Missing);
    assert!(matches!(error, Err(ProjectInputError::UnknownRequest(_))));
}

#[test]
fn rejected_duplicate_source_does_not_replace_the_original() {
    let linter = test_linter();
    let mut session = linter.begin_analysis("/project").unwrap();
    session
        .add_source(source_file("main.js", "fetch('/remote');"))
        .unwrap();
    let error = session.add_source(source_file("./main.js", ""));
    assert!(matches!(error, Err(ProjectInputError::DuplicateSource(_))));

    let report = session.finish().unwrap();
    assert_eq!(report.files[0].findings.len(), 1);
}

#[test]
fn type_only_reexports_do_not_create_runtime_requests() {
    let linter = test_linter();
    let mut session = linter.begin_analysis("/project").unwrap();
    let requests = session
        .add_source(source_file(
            "types.ts",
            "export { type Foo } from './dependency';",
        ))
        .unwrap();
    assert!(requests.is_empty());
}

#[test]
fn linker_accepts_named_reexports_and_reports_missing_exports() {
    let linter = test_linter();
    let mut project = ProjectFixture::new(&linter);
    project.add("dep.js", "export const value = 1;");
    project.add_resolved(
        "barrel.js",
        "export { value } from './dep';",
        [ResolutionResult::Internal {
            path: project_path("dep.js"),
        }],
    );
    project.add_resolved(
        "main.js",
        "import { value } from './barrel';",
        [ResolutionResult::Internal {
            path: project_path("barrel.js"),
        }],
    );
    let report = project.finish();
    assert!(
        report.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let mut missing = ProjectFixture::new(&linter);
    missing.add_resolved(
        "main.js",
        "import { nope } from './dep';",
        [ResolutionResult::Internal {
            path: project_path("dep.js"),
        }],
    );
    missing.add("dep.js", "export const value = 1;");
    let report = missing.finish();
    assert!(report.files.iter().any(|file| {
        file.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code() == "missing_imported_export")
    }));
}

#[test]
fn linker_reports_ambiguous_multiple_star_exports() {
    let linter = test_linter();
    let mut project = ProjectFixture::new(&linter);
    project.add("a.js", "export const value = 1;");
    project.add("b.js", "export const value = 2;");
    project.add_resolved(
        "barrel.js",
        "export * from './a'; export * from './b';",
        [
            ResolutionResult::Internal {
                path: project_path("a.js"),
            },
            ResolutionResult::Internal {
                path: project_path("b.js"),
            },
        ],
    );
    project.add_resolved(
        "main.js",
        "import { value } from './barrel';",
        [ResolutionResult::Internal {
            path: project_path("barrel.js"),
        }],
    );

    let report = project.finish();
    assert!(
        report
            .files
            .iter()
            .flat_map(|file| &file.diagnostics)
            .any(|diagnostic| diagnostic.code() == "ambiguous_star_export")
    );
}

#[test]
fn outside_project_targets_accept_normalized_absolute_paths() {
    let linter = test_linter();
    let mut project = ProjectFixture::new(&linter);
    project.add_resolved(
        "main.js",
        "import value from './outside';",
        [ResolutionResult::OutsideProject {
            path: "/other/./dependency.js".into(),
        }],
    );
    let report = project.finish();
    assert_eq!(
        report.files[0].diagnostics[0].code(),
        "outside_project_target"
    );
}

#[test]
fn dynamic_commonjs_export_shapes_are_reported_and_fail_closed() {
    let linter = test_linter();
    let mut project = ProjectFixture::new(&linter);
    project.add_resolved(
        "main.js",
        "import { value } from './dependency';",
        [ResolutionResult::Internal {
            path: project_path("dependency.js"),
        }],
    );
    project.add("dependency.js", "module.exports = { value: 1, ...extra };");

    let report = project.finish();
    assert!(
        report
            .files
            .iter()
            .flat_map(|file| &file.diagnostics)
            .any(|diagnostic| diagnostic.code() == "unsupported_commonjs_exports")
    );
}

mod status_policy {
    use super::*;
    use crate::AnalysisLimits;

    fn configured_linter(limits: AnalysisLimits) -> crate::Linter {
        test_linter_with_selection(crate::RuleSelection::default(), limits)
    }

    fn configured_flow_linter(limits: AnalysisLimits) -> crate::Linter {
        crate::Linter::new(
            crate::LinterConfig::new(
                vec![flow_linter().catalog().clone()],
                flow_linter().analysis_environment().clone(),
            )
            .with_limits(limits),
        )
        .unwrap()
    }

    fn lint_one(linter: &crate::Linter, path: &str, source: &str) -> AnalysisReport {
        let mut session = linter.begin_analysis("/project").unwrap();
        session.add_source(source_file(path, source)).unwrap();
        session.finish().unwrap()
    }

    fn diagnostics(report: &AnalysisReport) -> Vec<(Option<&str>, &str)> {
        report
            .files
            .iter()
            .flat_map(|file| {
                file.diagnostics
                    .iter()
                    .map(|diagnostic| (Some(file.path.as_str()), diagnostic.code()))
            })
            .chain(report.diagnostics.iter().map(|diagnostic| {
                (
                    diagnostic.path().map(ProjectRelativePath::as_str),
                    diagnostic.code(),
                )
            }))
            .collect()
    }

    fn request_report(result: ResolutionResult) -> AnalysisReport {
        let linter = test_linter();
        let mut fixture = ProjectFixture::new(&linter);
        fixture.add_resolved("main.js", "import value from './dep';", [result]);
        fixture.finish()
    }

    fn ambiguous_report() -> AnalysisReport {
        let linter = test_linter();
        let mut fixture = ProjectFixture::new(&linter);
        fixture.add("a.js", "export const value = 1;");
        fixture.add("b.js", "export const value = 2;");
        fixture.add_resolved(
            "barrel.js",
            "export * from './a'; export * from './b';",
            [
                ResolutionResult::Internal {
                    path: project_path("a.js"),
                },
                ResolutionResult::Internal {
                    path: project_path("b.js"),
                },
            ],
        );
        fixture.add_resolved(
            "main.js",
            "import { value } from './barrel';",
            [ResolutionResult::Internal {
                path: project_path("barrel.js"),
            }],
        );
        fixture.finish()
    }

    #[test]
    fn status_policy_matrix_has_expected_scope_and_completion() {
        let linter = test_linter();
        let syntax = lint_one(&linter, "syntax.js", "function {");
        let depth_limits = AnalysisLimits {
            syntax_depth: 1,
            ..AnalysisLimits::default()
        };
        let depth = lint_one(
            &configured_linter(depth_limits),
            "depth.js",
            "function f() { return (1); }",
        );
        let oversized = lint_one(
            &linter,
            "large.js",
            &"x".repeat(crate::MAX_SOURCE_BYTES + 1),
        );

        let mut dynamic = ProjectFixture::new(&linter);
        dynamic.add_resolved(
            "main.js",
            "import { value } from './dep';",
            [ResolutionResult::Internal {
                path: project_path("dep.js"),
            }],
        );
        dynamic.add("dep.js", "module.exports = { value: 1, ...extra };");
        let dynamic = dynamic.finish();
        let missing = request_report(ResolutionResult::Missing);
        let unsupported = request_report(ResolutionResult::Unsupported {
            reason: "unsupported extension".into(),
        });
        let outside = request_report(ResolutionResult::OutsideProject {
            path: "/other/dep.js".into(),
        });
        let ambiguous = ambiguous_report();

        let rows = [
            (&syntax, Some("syntax.js"), "syntax_error"),
            (&depth, Some("depth.js"), "syntax_depth_exceeded"),
            (&oversized, Some("large.js"), "source_too_large"),
            (&dynamic, Some("dep.js"), "unsupported_commonjs_exports"),
            (&missing, Some("main.js"), "unresolved_internal_request"),
            (&unsupported, Some("main.js"), "unsupported_project_target"),
            (&outside, Some("main.js"), "outside_project_target"),
            (&ambiguous, Some("main.js"), "ambiguous_star_export"),
        ];
        for (report, path, code) in rows {
            assert_eq!(report.completion, ReportCompletion::Partial, "{code}");
            assert_eq!(diagnostics(report), vec![(path, code)], "{code}");
            assert!(
                report.files.iter().all(|file| file.findings.is_empty()),
                "{code}"
            );
        }
    }

    #[test]
    fn parse_status_and_structured_diagnostic_stay_consistent() {
        let report = lint_one(&test_linter(), "broken.js", "fetch(");
        assert_eq!(report.completion, ReportCompletion::Partial);
        let diagnostic = &report.files[0].diagnostics[0];
        assert!(diagnostic.parse_diagnostic().is_some());
        assert_eq!(diagnostic.path().unwrap().as_str(), "broken.js");
        let restored: AnalysisReport =
            serde_json::from_str(&serde_json::to_string(&report).unwrap()).unwrap();
        assert_eq!(restored.completion, ReportCompletion::Partial);
        assert!(
            restored.files[0].diagnostics[0]
                .parse_diagnostic()
                .is_some()
        );
        assert_eq!(restored, report);
    }

    #[test]
    fn external_and_builtin_requests_are_complete() {
        for result in [
            ResolutionResult::External {
                package: "package".into(),
            },
            ResolutionResult::Builtin {
                name: "node:fs".into(),
            },
        ] {
            let report = request_report(result);
            assert_eq!(report.completion, ReportCompletion::Complete);
            assert!(diagnostics(&report).is_empty());
        }
    }

    #[test]
    fn proven_missing_export_is_diagnostic_but_complete() {
        let linter = test_linter();
        let mut fixture = ProjectFixture::new(&linter);
        fixture.add_resolved(
            "main.js",
            "import { missing } from './dep';",
            [ResolutionResult::Internal {
                path: project_path("dep.js"),
            }],
        );
        fixture.add("dep.js", "export const present = 1;");
        let report = fixture.finish();
        assert_eq!(report.completion, ReportCompletion::Complete);
        assert_eq!(
            diagnostics(&report),
            vec![(Some("main.js"), "missing_imported_export")]
        );
    }

    fn assert_limit_triplet(
        configure: fn(AnalysisLimits) -> crate::Linter,
        component: fn(&mut AnalysisLimits) -> &mut usize,
        required: usize,
        code: &str,
        scope: Option<&str>,
        analyze: impl Fn(&crate::Linter) -> AnalysisReport,
    ) {
        assert!(required > 1);
        for (limit, expected) in [
            (required - 1, ReportCompletion::Partial),
            (required, ReportCompletion::Complete),
            (required + 1, ReportCompletion::Complete),
        ] {
            let mut limits = AnalysisLimits::default();
            *component(&mut limits) = limit;
            let report = analyze(&configure(limits));
            assert_eq!(report.completion, expected, "{code} limit={limit}");
            if expected == ReportCompletion::Partial {
                let status_diagnostics = diagnostics(&report)
                    .into_iter()
                    .filter(|(_, actual)| *actual == code)
                    .collect::<Vec<_>>();
                assert_eq!(status_diagnostics, vec![(scope, code)]);
                assert!(report.files.iter().all(|file| file.findings.is_empty()));
            } else {
                assert!(
                    diagnostics(&report)
                        .iter()
                        .all(|(_, actual)| *actual != code)
                );
            }
        }
    }

    #[test]
    fn facts_effects_flow_and_link_limits_cover_below_at_above() {
        let facts = |linter: &crate::Linter| {
            lint_one(
                linter,
                "main.js",
                "function f(value) { return value; } f(1);",
            )
        };
        assert_limit_triplet(
            configured_linter,
            |limits| &mut limits.semantic_operations,
            7,
            "semantic_budget_exhausted",
            Some("main.js"),
            facts,
        );

        let effects = |linter: &crate::Linter| {
            lint_one(linter, "main.js", "function f(value) { return value; }")
        };
        assert_limit_triplet(
            configured_linter,
            |limits| &mut limits.effect_operations,
            3,
            "effect_size_budget_exhausted",
            Some("main.js"),
            effects,
        );

        let linking = |linter: &crate::Linter| {
            let mut fixture = ProjectFixture::new(linter);
            fixture.add_resolved(
                "main.js",
                "import { value } from './dep';",
                [ResolutionResult::Internal {
                    path: project_path("dep.js"),
                }],
            );
            fixture.add("dep.js", "export const value = 1; export const other = 2;");
            fixture.finish()
        };
        assert_limit_triplet(
            configured_linter,
            |limits| &mut limits.link_operations,
            2,
            "graph_link_budget_exhausted",
            None,
            linking,
        );

        let flow = |linter: &crate::Linter| {
            let mut fixture = ProjectFixture::new(linter);
            fixture.add(
                "helper.js",
                "export function append(element) { element.src = url; document.head.appendChild(element); }",
            );
            fixture.add_resolved(
                "main.js",
                "import { append } from './helper'; const element = document.createElement('script'); append(element);",
                [ResolutionResult::Internal {
                    path: project_path("helper.js"),
                }],
            );
            fixture.finish()
        };
        assert_limit_triplet(
            configured_flow_linter,
            |limits| &mut limits.flow_operations,
            2,
            "flow_link_budget_exhausted",
            None,
            flow,
        );
    }

    #[test]
    fn partial_status_never_emits_unproved_strict_finding() {
        let rule = Rule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("./dep", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(crate::LinterConfig::new(
            vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
            crate::Environment::default(),
        ))
        .unwrap();
        let report = lint_one(
            &linter,
            "main.js",
            "import { request } from './dep'; request();",
        );
        assert_eq!(report.completion, ReportCompletion::Partial);
        assert!(report.files[0].findings.is_empty());
        assert_eq!(
            diagnostics(&report),
            vec![(Some("main.js"), "unresolved_internal_request")]
        );
    }
}
