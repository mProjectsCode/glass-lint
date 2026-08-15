use std::sync::atomic::{AtomicUsize, Ordering};

use glass_lint_core::Severity;

use super::*;

struct FakeBundler(AtomicUsize);

impl Bundler for FakeBundler {
    fn bundle(
        &self,
        _request: &crate::bundler::BundleRequest,
    ) -> anyhow::Result<crate::bundler::BundleOutput> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(crate::bundler::BundleOutput {
            transformer_version: "fake-1".into(),
            source: "var value = 1; globalThis.value = value;".into(),
            bytes: 42,
            digest: "fake-digest".into(),
        })
    }
}

fn finding() -> Finding {
    let location = glass_lint_core::project::SourceLocation::new(
        glass_lint_core::project::ProjectRelativePath::new("main.js").unwrap(),
        glass_lint_datastructures::SourceRange::new(
            glass_lint_datastructures::Position::new(2, 3).unwrap(),
            glass_lint_datastructures::Position::new(2, 4).unwrap(),
        )
        .unwrap(),
    );
    Finding::new(
        glass_lint_core::RuleId::parse("test:a.b").unwrap(),
        "text".into(),
        Severity::Warning,
        location.clone(),
        glass_lint_core::project::EvidenceTraces::fallback(location),
        glass_lint_core::project::MatchCertainty::Definite,
    )
}

#[test]
fn finds_missing_diagnostic() {
    let mut expected = ToolExpectation::new(None, vec!["test:a.b".into()]).unwrap();
    expected.add_required(FindingExpectation {
        path: None,
        rule_id: glass_lint_core::RuleId::parse("test:a.b").unwrap(),
        severity: None,
        count: ExpectedCount::Exactly(2),
        line: None,
        column: None,
        message: None,
        certainty: None,
    });
    assert_eq!(compare(&[finding()], &expected).len(), 1);
}

#[test]
fn flags_unexpected_diagnostic() {
    let expected = ToolExpectation::new(Some("heuristic".into()), Vec::new()).unwrap();
    assert_eq!(compare(&[finding()], &expected).len(), 1);
}

#[test]
fn reports_forbidden_diagnostic_once() {
    let mut expected = ToolExpectation::new(None, vec!["test:a.b".into()]).unwrap();
    expected.add_forbidden(FindingExpectation {
        path: None,
        rule_id: glass_lint_core::RuleId::parse("test:a.b").unwrap(),
        severity: None,
        count: ExpectedCount::Exactly(1),
        line: None,
        column: None,
        message: None,
        certainty: None,
    });
    assert_eq!(compare(&[finding()], &expected).len(), 1);
}

#[test]
fn count_comparison_treats_missing_and_new_rules_as_zero_and_mismatch() {
    let key = BundleKey {
        profile: crate::types::BundleProfile::Web,
        transformer: BundleTransformer::Esbuild,
        minified: true,
        target: BundleTarget::Es2022,
    };
    let authored = BTreeMap::from([(String::from("js:old"), 1usize)]);
    let transformed = BTreeMap::from([(String::from("js:new"), 2usize)]);
    let mismatches = compare_rule_counts(&authored, &transformed, &key);
    assert_eq!(mismatches.len(), 2);
    assert!(mismatches[0].contains("before=0") || mismatches[1].contains("before=0"));
    assert!(mismatches[0].contains("after=0") || mismatches[1].contains("after=0"));
}

#[test]
fn fake_bundler_runs_the_complete_selected_matrix() {
    let mut case = crate::types::Case::new(
        "bundled",
        "bundled",
        "javascript",
        "main.js",
        "var value = 1;",
    )
    .unwrap()
    .with_tool(
        "glass-lint",
        ToolExpectation::new(None, vec!["obsidian:network.request".into()]).unwrap(),
    )
    .unwrap();
    case.set_bundles(vec![
        crate::types::BundleProfile::Web,
        crate::types::BundleProfile::Obsidian,
    ]);
    let mut tools = BTreeMap::new();
    tools.insert(
        "glass-lint".into(),
        ToolResult {
            version: "test".into(),
            skipped: false,
            skip_reason: None,
            passed: false,
            findings: Vec::new(),
            mismatches: vec!["ordinary expectation failure".into()],
            operational_errors: Vec::new(),
        },
    );
    let bundler = FakeBundler(AtomicUsize::new(0));
    let (results, timings) = run_bundles(&case, &tools, &bundler);
    assert_eq!(results.len(), 40);
    assert_eq!(timings.len(), 40);
    assert!(results.iter().all(|result| result.passed));
    assert_eq!(bundler.0.load(Ordering::Relaxed), 40);
}
