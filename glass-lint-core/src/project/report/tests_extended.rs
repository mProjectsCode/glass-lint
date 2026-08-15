use super::*;

#[test]
fn direct_qualification_matches_one_file_project_shape() {
    let rule = Rule::catalog_builder("network.request")
        .description("Uses fetch")
        .severity(RuleSeverity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    let mut environment = crate::Environment::default();
    environment.add_global("fetch").unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap();
    let source = "fetch(\"https://example.test\");";
    let direct = linter
        .lint_source(source_file("main.js", source))
        .unwrap()
        .files()[0]
        .clone();
    let mut manual_session = linter.begin_project();
    manual_session
        .analyze_source(source_file("main.js", source))
        .unwrap();
    let manual = manual_session.finish([]).unwrap().into_report();
    assert_eq!(direct, manual.files()[0].clone());
}

#[cfg(feature = "serde")]
#[test]
fn report_is_source_free_and_not_serialized() {
    let file = FileReport::new(
        ProjectRelativePath::new("main.js").unwrap(),
        Vec::new(),
        Vec::new(),
    );

    let json = serde_json::to_value(&file).unwrap();
    assert!(json.get("source").is_none());
}

#[cfg(feature = "serde")]
#[test]
fn match_certainty_serializes_as_stable_spellings() {
    let definite = serde_json::to_value(MatchCertainty::Definite).unwrap();
    let possible = serde_json::to_value(MatchCertainty::Possible).unwrap();
    assert_eq!(definite, "definite");
    assert_eq!(possible, "possible");
}

#[cfg(feature = "serde")]
#[test]
fn finding_serialization_includes_certainty() {
    let finding = Finding::new(
        RuleId::parse("js:network.request").unwrap(),
        "test".into(),
        Severity::Warning,
        SourceLocation::new(ProjectRelativePath::new("main.js").unwrap(), range(1, 1, 2)),
        EvidenceTraces::new(vec![
            EvidenceTrace::new(vec![EvidenceStep::new(
                EvidenceRole::Occurrence,
                "test evidence".into(),
                SourceLocation::new(ProjectRelativePath::new("main.js").unwrap(), range(1, 1, 2)),
            )])
            .unwrap(),
        ])
        .unwrap(),
        MatchCertainty::Definite,
    );
    let json = serde_json::to_value(&finding).unwrap();
    assert_eq!(json["certainty"], "definite");

    let possible = Finding::new(
        RuleId::parse("js:network.request").unwrap(),
        "test".into(),
        Severity::Warning,
        SourceLocation::new(ProjectRelativePath::new("main.js").unwrap(), range(1, 1, 2)),
        EvidenceTraces::new(vec![
            EvidenceTrace::new(vec![EvidenceStep::new(
                EvidenceRole::Occurrence,
                "test evidence".into(),
                SourceLocation::new(ProjectRelativePath::new("main.js").unwrap(), range(1, 1, 2)),
            )])
            .unwrap(),
        ])
        .unwrap(),
        MatchCertainty::Possible,
    );
    let json = serde_json::to_value(&possible).unwrap();
    assert_eq!(json["certainty"], "possible");
}

#[cfg(feature = "serde")]
#[test]
fn snippet_serializes_as_one_analysis_file_without_source_text() {
    let rule = Rule::catalog_builder("network.request")
        .description("Uses fetch")
        .severity(RuleSeverity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    let mut environment = crate::Environment::default();
    environment.add_global("fetch").unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap();
    let report = linter
        .lint_source(source_file("main.js", "fetch('/');"))
        .unwrap();
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["files"].as_array().unwrap().len(), 1);
    assert!(json["files"][0].get("source").is_none());
    assert_eq!(
        json["files"][0]["findings"][0]["location"]["path"],
        "main.js"
    );
}

#[test]
fn parse_and_valid_sources_each_produce_one_file_report() {
    let rule = Rule::catalog_builder("network.request")
        .description("Uses fetch")
        .severity(RuleSeverity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    let mut environment = crate::Environment::default();
    environment.add_global("fetch").unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap();

    // One valid file, one parse-failure file
    let mut collection = linter.begin_project();
    collection
        .analyze_source(source_file("valid.js", "fetch('/a');"))
        .unwrap();
    collection
        .analyze_source(source_file("broken.js", "fetch("))
        .unwrap();
    let report = collection.finish([]).unwrap().into_report();

    assert_eq!(report.files().len(), 2);
    let valid = report
        .files()
        .iter()
        .find(|f| f.path().as_str() == "valid.js")
        .unwrap();
    let broken = report
        .files()
        .iter()
        .find(|f| f.path().as_str() == "broken.js")
        .unwrap();

    assert_eq!(valid.findings().len(), 1);
    assert_eq!(valid.diagnostics().len(), 0);

    assert_eq!(broken.findings().len(), 0);
    assert!(broken.has_parse_diagnostics());
}
