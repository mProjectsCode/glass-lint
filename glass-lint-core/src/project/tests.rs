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
        session::{ControlledReleaseOrder, CountingExecutionObserver, outstanding_job_bound},
        *,
    },
};

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

#[path = "tests/support.rs"]
mod support;
pub use support::*;

#[path = "tests/cache_and_session.rs"]
mod cache_and_session;
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
