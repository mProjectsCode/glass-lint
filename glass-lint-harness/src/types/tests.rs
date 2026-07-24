use super::*;

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
