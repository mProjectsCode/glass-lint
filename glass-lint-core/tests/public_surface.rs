use glass_lint_core::{
    AnalysisLimits, ByteRange, DiagnosticCode, Environment, InvalidPosition,
    InvalidSourcePositionRange, Linter, Position, ProjectInput, Rule, RuleCatalog, Severity,
    SourceFile, SourceRange,
    rules::{CallMatcher, Confidence},
};

#[test]
fn supported_public_operations_do_not_require_engine_storage() {
    let rule = Rule::builder("network.fetch")
        .label("Uses fetch")
        .category("network")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(CallMatcher::global("fetch"))
        .build()
        .unwrap();
    let mut environment = Environment::default();
    environment.add_global("fetch").unwrap();
    let catalog = RuleCatalog::new("test", vec![rule]).unwrap();
    let linter = Linter::new(
        glass_lint_core::LinterConfig::new(vec![catalog], environment)
            .with_limits(AnalysisLimits::default()),
    )
    .unwrap();
    let report = linter
        .lint_project(ProjectInput {
            root: "/project".into(),
            sources: vec![SourceFile::new("main.js", "fetch('/remote');").unwrap()],
            resolutions: Vec::new(),
        })
        .unwrap();
    assert_eq!(report.files.len(), 1);
    assert_eq!(report.files[0].findings.len(), 1);
}

#[test]
fn public_invariant_types_reject_invalid_values_without_panicking() {
    let range = std::panic::catch_unwind(|| ByteRange::new(4, 3));
    assert!(range.is_ok());
    assert_eq!(
        range.unwrap().unwrap_err().to_string(),
        "byte range start exceeds end"
    );
    assert!(serde_json::from_str::<ByteRange>(r#"{"start":4,"end":3}"#).is_err());
    assert_eq!(
        serde_json::to_string(&ByteRange::new(2, 5).unwrap()).unwrap(),
        r#"{"start":2,"end":5}"#
    );

    let max_length = "a".repeat(64);
    let too_long = "a".repeat(65);
    let cases = [
        ("", false),
        ("valid", true),
        ("valid_code2", true),
        (max_length.as_str(), true),
        (too_long.as_str(), false),
        ("UPPER", false),
        ("aUpper", false),
        ("0bad", false),
        ("_bad", false),
        ("bad.code", false),
        ("bad-code", false),
        ("café", false),
    ];
    for (candidate, valid) in cases {
        let direct = std::panic::catch_unwind(|| DiagnosticCode::try_from(candidate));
        assert!(direct.is_ok(), "{candidate:?}");
        if valid {
            assert_eq!(direct.unwrap().unwrap().as_str(), candidate);
        } else {
            assert_eq!(direct.unwrap(), Err(candidate.to_string()));
        }
        let decoded = serde_json::from_str::<DiagnosticCode>(&format!(r#""{candidate}""#));
        assert_eq!(decoded.is_ok(), valid, "{candidate:?}");
    }
    let code = DiagnosticCode::try_from("valid_code2").unwrap();
    assert_eq!(serde_json::to_string(&code).unwrap(), r#""valid_code2""#);
    assert_eq!(
        serde_json::from_str::<DiagnosticCode>(&serde_json::to_string(&code).unwrap()).unwrap(),
        code
    );

    let zero_line = std::panic::catch_unwind(|| Position::new(0, 1));
    assert_eq!(zero_line.unwrap(), Err(InvalidPosition::ZeroLine));
    let zero_column = std::panic::catch_unwind(|| Position::new(1, 0));
    assert_eq!(zero_column.unwrap(), Err(InvalidPosition::ZeroColumn));
    assert!(serde_json::from_str::<Position>(r#"{"line":0,"column":1}"#).is_err());
    assert!(serde_json::from_str::<Position>(r#"{"line":1,"column":0}"#).is_err());

    let start = Position::new(2, 3).unwrap();
    let end = Position::new(2, 8).unwrap();
    let reversed = std::panic::catch_unwind(|| SourceRange::new(end.clone(), start.clone()));
    assert_eq!(reversed.unwrap(), Err(InvalidSourcePositionRange));
    assert!(
        serde_json::from_str::<SourceRange>(
            r#"{"start":{"line":2,"column":8},"end":{"line":2,"column":3}}"#
        )
        .is_err()
    );
    let source_range = SourceRange::new(start, end).unwrap();
    let json = r#"{"start":{"line":2,"column":3},"end":{"line":2,"column":8}}"#;
    assert_eq!(serde_json::to_string(&source_range).unwrap(), json);
    assert_eq!(
        serde_json::from_str::<SourceRange>(json).unwrap(),
        source_range
    );
    assert_eq!(
        (source_range.start().line(), source_range.start().column()),
        (2, 3)
    );
    assert_eq!(
        (source_range.end().line(), source_range.end().column()),
        (2, 8)
    );
}
