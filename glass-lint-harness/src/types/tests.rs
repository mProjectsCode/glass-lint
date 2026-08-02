use super::*;

#[test]
fn validation_errors_keep_their_domain() {
    assert_eq!(
        Case::new("", "", "javascript", "main.js", "").unwrap_err(),
        CaseError::EmptyIdentity
    );
    assert_eq!(
        ToolExpectation::new(None, Vec::new()).unwrap_err(),
        ExpectationError::InvalidSelector
    );
    assert!(matches!(
        FindingExpectation::new("not-a-rule-id"),
        Err(FindingExpectationError::InvalidRuleId(_))
    ));
    assert!(matches!(
        FindingExpectation::new("js:a.b")
            .unwrap()
            .qualify_for_file("../outside.js"),
        Err(glass_lint_core::project::ProjectInputError::InvalidPath(_))
    ));
    assert!(matches!(
        <&AdapterResolution as TryInto<(_, _)>>::try_into(&resolution(
            AdapterResolutionKind::Import,
            AdapterResolutionResult::Internal {
                path: "../outside.js".into(),
            },
        )),
        Err(AdapterConversionError::InvalidInternalPath(_))
    ));
}

#[test]
fn qualify_for_file_defaults_missing_required_and_forbidden_paths() {
    let mut expectation = ToolExpectation::new(None, vec!["js:a.b".into()]).unwrap();
    expectation.add_required(FindingExpectation::new("js:a.b").unwrap());
    expectation.add_forbidden(
        FindingExpectation::new("js:a.b")
            .unwrap()
            .with_path("lib.js")
            .unwrap(),
    );

    let qualified = expectation.qualify_for_file("src/main.js").unwrap();
    assert_eq!(
        qualified.required()[0]
            .path
            .as_ref()
            .map(glass_lint_core::project::ProjectRelativePath::as_str),
        Some("src/main.js")
    );
    assert_eq!(
        qualified.forbidden()[0]
            .path
            .as_ref()
            .map(glass_lint_core::project::ProjectRelativePath::as_str),
        Some("lib.js")
    );
}

fn resolution(kind: AdapterResolutionKind, result: AdapterResolutionResult) -> AdapterResolution {
    AdapterResolution {
        importer: "main.js".into(),
        kind,
        request: "request".into(),
        range: glass_lint_datastructures::SourceRange::new(
            glass_lint_datastructures::Position::new(1, 2).unwrap(),
            glass_lint_datastructures::Position::new(1, 8).unwrap(),
        )
        .unwrap(),
        result,
    }
}

#[test]
fn adapter_project_protocol_json_preserves_all_resolution_variants() {
    let project = AdapterProject {
        root: "/tmp/project".into(),
        entries: vec!["main.js".into()],
        files: vec![AdapterFile {
            path: "main.js".into(),
            language: "javascript".into(),
            source: "import x from 'x';".into(),
        }],
        resolutions: vec![
            resolution(
                AdapterResolutionKind::Import,
                AdapterResolutionResult::Internal {
                    path: "lib.js".into(),
                },
            ),
            resolution(
                AdapterResolutionKind::DynamicImport,
                AdapterResolutionResult::External {
                    package: "pkg".into(),
                },
            ),
            resolution(
                AdapterResolutionKind::Require,
                AdapterResolutionResult::Builtin { name: "fs".into() },
            ),
            resolution(
                AdapterResolutionKind::Import,
                AdapterResolutionResult::Missing,
            ),
            resolution(
                AdapterResolutionKind::DynamicImport,
                AdapterResolutionResult::OutsideProject {
                    path: "../outside.js".into(),
                },
            ),
            resolution(
                AdapterResolutionKind::Require,
                AdapterResolutionResult::Unsupported {
                    reason: "dynamic target".into(),
                },
            ),
        ],
    };
    let json = serde_json::to_value(&project).unwrap();
    assert_eq!(json["resolutions"][0]["kind"], "import");
    assert_eq!(json["resolutions"][1]["kind"], "dynamic_import");
    assert_eq!(json["resolutions"][2]["kind"], "require");
    assert_eq!(json["resolutions"][0]["result"]["kind"], "internal");
    assert_eq!(json["resolutions"][1]["result"]["kind"], "external");
    assert_eq!(json["resolutions"][2]["result"]["kind"], "builtin");
    assert_eq!(json["resolutions"][3]["result"]["kind"], "missing");
    assert_eq!(json["resolutions"][4]["result"]["kind"], "outside_project");
    assert_eq!(json["resolutions"][5]["result"]["kind"], "unsupported");
}

#[test]
fn adapter_project_round_trips_protocol_data() {
    let project = AdapterProject {
        root: "/tmp/project".into(),
        entries: vec!["main.js".into()],
        files: vec![AdapterFile {
            path: "main.js".into(),
            language: "javascript".into(),
            source: "fetch('/');".into(),
        }],
        resolutions: Vec::new(),
    };
    let encoded = serde_json::to_string(&project).unwrap();
    let decoded: AdapterProject = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, project);
}

fn adapter_response_json() -> serde_json::Value {
    serde_json::json!({
        "protocol_version": ADAPTER_PROTOCOL_VERSION,
        "tool": "external",
        "tool_version": "1.0.0",
        "findings": [{
            "rule_id": "js:network.request",
            "message": "Makes a request",
            "severity": "warning",
            "location": {
                "path": "main.js",
                "range": {
                    "start": {"line": 1, "column": 1},
                    "end": {"line": 1, "column": 6}
                }
            },
            "certainty": "definite",
            "evidence": {
                "traces": [{
                    "steps": [{
                        "role": "occurrence",
                        "message": "match",
                        "location": {
                            "path": "main.js",
                            "range": {
                                "start": {"line": 1, "column": 1},
                                "end": {"line": 1, "column": 6}
                            }
                        }
                    }]
                }]
            }
        }]
    })
}

#[test]
fn adapter_response_requires_certainty_and_traces() {
    let valid = adapter_response_json();
    let decoded: AdapterResponse = serde_json::from_value(valid.clone()).unwrap();
    let encoded = serde_json::to_value(decoded).unwrap();
    assert_eq!(encoded["findings"][0]["rule_id"], "js:network.request");
    assert_eq!(
        encoded["findings"][0]["evidence"]["traces"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let mut missing_certainty = valid.clone();
    missing_certainty["findings"][0]
        .as_object_mut()
        .unwrap()
        .remove("certainty");
    assert!(serde_json::from_value::<AdapterResponse>(missing_certainty).is_err());

    let mut missing_traces = valid;
    missing_traces["findings"][0]
        .as_object_mut()
        .unwrap()
        .remove("evidence");
    assert!(serde_json::from_value::<AdapterResponse>(missing_traces).is_err());
}

#[test]
fn adapter_response_rejects_empty_evidence() {
    let mut response = adapter_response_json();
    response["findings"][0]["evidence"]["traces"] = serde_json::json!([]);
    assert!(serde_json::from_value::<AdapterResponse>(response).is_err());
}
