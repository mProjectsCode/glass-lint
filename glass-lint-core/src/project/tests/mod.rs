//! Project-session integration tests for normalization, linking, flow, and
//! report ownership across multiple authored source files.

use glass_lint_datastructures::{Position, SourceRange};

use crate::{
    api::rule::{
        Confidence, LifecycleCompletion, LifecycleCondition, LifecycleEvent, LifecycleQuery,
        LifecycleSink, Rule, Severity, ValueMatcher,
    },
    project::{
        session::{CountingExecutionObserver, outstanding_job_bound},
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
        let mut session = linter.begin_project();
        session
            .analyze_sources(
                sources.iter().cloned(),
                std::num::NonZeroUsize::new(workers).unwrap_or(std::num::NonZeroUsize::MIN),
            )
            .unwrap();
        let report = session.finish([]).unwrap().into_report();
        reports.push(report);
    }
    assert!(reports.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn consuming_project_phases_validate_requests_at_the_boundary() {
    let linter = test_linter();
    let mut collection = linter.begin_project();
    let analysis = collection
        .analyze_source(source_file(
            "main.js",
            "import value from './dep.js'; value();",
        ))
        .unwrap();
    assert_eq!(analysis.iter().len(), 1);
    let key = analysis.iter().next().unwrap().key().clone();
    let report = collection
        .finish([(key, crate::project::ResolverOutcome::Missing)])
        .unwrap()
        .into_report();
    assert_eq!(report.files().len(), 1);
}

#[test]
fn finish_rejects_admitted_sources_without_analysis_outcomes() {
    let linter = test_linter();
    let mut collection = linter.begin_project();
    collection
        .accept_test_source(source_file("pending.js", "fetch('/pending');"))
        .unwrap();

    let Err(error) = collection.finish([]) else {
        panic!("finish must reject sources without an analysis outcome")
    };
    assert_eq!(
        error,
        ProjectError::Phase(ProjectPhaseError::IncompleteLocalAnalysis(vec![
            project_path("pending.js",)
        ]))
    );
}

#[test]
fn consuming_resolution_rejects_unknown_and_duplicate_outcomes() {
    let linter = test_linter();
    let mut collection = linter.begin_project();
    let analysis = collection
        .analyze_source(source_file(
            "main.js",
            "import value from './dep.js'; value();",
        ))
        .unwrap();
    let key = analysis.iter().next().unwrap().key().clone();
    let mut unknown = key;
    unknown = crate::project::ResolutionRequestKey::new(
        unknown.importer().clone(),
        crate::project::ResolutionRequestKind::Require,
        unknown.range(),
    );
    let Err(error) = collection.finish([(unknown, crate::project::ResolverOutcome::Missing)])
    else {
        panic!("unknown requests must be rejected")
    };
    assert!(matches!(
        error,
        crate::project::ProjectError::Phase(crate::project::ProjectPhaseError::UnknownRequest(_))
    ));

    let mut collection = linter.begin_project();
    let analysis = collection
        .analyze_source(source_file(
            "main.js",
            "import value from './dep.js'; value();",
        ))
        .unwrap();
    let key = analysis.iter().next().unwrap().key().clone();
    let Err(error) = collection.finish([
        (key.clone(), crate::project::ResolverOutcome::Missing),
        (key, crate::project::ResolverOutcome::Missing),
    ]) else {
        panic!("duplicate outcomes must be rejected")
    };
    assert!(matches!(
        error,
        crate::project::ProjectError::Phase(
            crate::project::ProjectPhaseError::DuplicateResolution(_),
        )
    ));
}

#[cfg(feature = "serde")]
#[test]
fn controlled_release_orders_produce_identical_full_report() {
    use crate::project::session::ControlledReleaseOrder;

    let limits = crate::AnalysisLimits::default()
        .with_semantic_operations(40)
        .unwrap();
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
        let mut session = linter.begin_project();
        session
            .analyze_sources_controlled(sources.iter().cloned(), 2, order)
            .unwrap();
        reports.push(serde_json::to_value(session.finish([]).unwrap().into_report()).unwrap());
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
            .any(|item| item["code"] == "semantic_step_budget_exhausted")
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
        let mut session = linter.begin_project();
        let sources = (0..12).map(|index| source_file(format!("{index:02}.js"), "fetch('x');"));
        let observer = CountingExecutionObserver::new();
        session
            .analyze_sources_counted(sources, requested, &observer)
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

#[cfg(feature = "serde")]
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

mod support;
pub use support::*;

mod cache_and_session;
mod input_validation;

#[test]
fn session_uses_project_analysis_and_preserves_single_file_findings() {
    let linter = test_linter();
    let source = "fetch('/remote');\n";
    let direct = linter.lint_source(source_file("a.js", source)).unwrap();

    let mut session = linter.begin_project();
    session.analyze_source(source_file("a.js", source)).unwrap();
    let project = session.finish([]).unwrap().into_report();

    assert_eq!(project.files().len(), 1);
    assert_eq!(project.files()[0].path().as_str(), "a.js");
    assert_eq!(
        project.files()[0].findings().len(),
        direct.files()[0].findings().len()
    );
    assert_eq!(
        project.files()[0].findings()[0].location().range(),
        direct.files()[0].findings()[0].location().range()
    );
    assert_eq!(
        project.files()[0].findings()[0].location().path().as_str(),
        "a.js"
    );
    assert_eq!(
        project.files()[0].findings()[0].evidence().traces()[0].steps()[0]
            .location()
            .path()
            .as_str(),
        "a.js"
    );
}

mod linking_and_flow;
mod session_and_link_validation;
mod status_policy;
