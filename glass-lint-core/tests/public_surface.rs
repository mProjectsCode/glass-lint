use glass_lint_core::{
    AnalysisLimits, Environment, Linter, Rule, RuleCatalog, Severity,
    project::{DiagnosticCode, SourceFile},
    rules::{Category, Confidence, MatcherDecl},
};
use glass_lint_datastructures::{
    ByteRange, InvalidPosition, Position, ReversedSourcePositionRange, SourceRange,
};

#[test]
fn supported_public_operations_do_not_require_engine_storage() {
    let rule = Rule::builder("network.fetch")
        .description("Uses fetch")
        .category(Category::new("network").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::builder().call_global("fetch").build().unwrap())
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
    let mut session = linter.begin_project("/project").unwrap();
    session
        .analyze_source(SourceFile::new("main.js", "fetch('/remote');").unwrap())
        .unwrap();
    let report = session
        .finish_local()
        .resolve([])
        .unwrap()
        .finish()
        .unwrap();
    assert_eq!(report.files().len(), 1);
    assert_eq!(report.files()[0].findings().len(), 1);
}

#[test]
fn public_invariant_types_reject_invalid_values_without_panicking() {
    let range = std::panic::catch_unwind(|| ByteRange::new(4, 3));
    assert!(range.is_ok());
    assert_eq!(
        range.unwrap().unwrap_err().to_string(),
        "byte range start exceeds end"
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
        let decoded = DiagnosticCode::try_from(candidate);
        assert_eq!(decoded.is_ok(), valid, "{candidate:?}");
    }
    let code = DiagnosticCode::try_from("valid_code2").unwrap();
    assert_eq!(DiagnosticCode::try_from(code.as_str()).unwrap(), code);

    let zero_line = std::panic::catch_unwind(|| Position::new(0, 1));
    assert_eq!(zero_line.unwrap(), Err(InvalidPosition::ZeroLine));
    let zero_column = std::panic::catch_unwind(|| Position::new(1, 0));
    assert_eq!(zero_column.unwrap(), Err(InvalidPosition::ZeroColumn));

    let a = Position::new(2, 3).unwrap();
    let b = Position::new(2, 8).unwrap();
    let reversed = std::panic::catch_unwind(|| SourceRange::new(b, a));
    assert_eq!(reversed.unwrap(), Err(ReversedSourcePositionRange));
}

#[cfg(feature = "serde")]
#[test]
fn serde_round_trips_validate_serialization_and_deserialization() {
    assert!(serde_json::from_str::<ByteRange>(r#"{"start":4,"end":3}"#).is_err());
    assert_eq!(
        serde_json::to_string(&ByteRange::new(2, 5).unwrap()).unwrap(),
        r#"{"start":2,"end":5}"#
    );

    let code = DiagnosticCode::try_from("valid_code2").unwrap();
    assert_eq!(serde_json::to_string(&code).unwrap(), r#""valid_code2""#);

    assert!(serde_json::from_str::<Position>(r#"{"line":0,"column":1}"#).is_err());
    assert!(serde_json::from_str::<Position>(r#"{"line":1,"column":0}"#).is_err());

    let start = Position::new(2, 3).unwrap();
    let end = Position::new(2, 8).unwrap();
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
}
