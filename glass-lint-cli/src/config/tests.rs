use super::*;

#[test]
fn obsidian_profile_combines_generic_and_provider_rules() {
    let linter = base_linter(Provider::Obsidian, RuleSelectionProfile::Heuristic);
    let mut ids = linter.catalog().rule_ids();

    assert!(ids.any(|id| id.as_str() == "js:dynamic-code.eval"));
    assert!(
        linter
            .catalog()
            .rule_ids()
            .any(|id| id.as_str() == "obsidian:markdown.code-block-processor")
    );
}

#[test]
fn combined_obsidian_profile_uses_the_obsidian_host_environment() {
    let report = base_linter(Provider::Obsidian, RuleSelectionProfile::Heuristic)
        .lint_source(
            SourceFile::new(
                "render-executable-code-blocks.js",
                include_str!("../../../tests/e2e/render-executable-code-blocks.js"),
            )
            .unwrap(),
        )
        .unwrap();
    let evals = report
        .files()
        .iter()
        .flat_map(|file| file.findings().iter())
        .filter(|finding| finding.rule_id().as_str() == "js:dynamic-code.eval")
        .count();
    let processors = report
        .files()
        .iter()
        .flat_map(|file| file.findings().iter())
        .filter(|finding| finding.rule_id().as_str() == "obsidian:markdown.code-block-processor")
        .count();

    assert_eq!(evals, 2);
    assert_eq!(processors, 2);
}

#[test]
fn selected_linter_keeps_profile_baseline_before_core_overrides() {
    let mut recommended = Config::default();
    recommended.cli.provider = Provider::Js;
    recommended.cli.profile = RuleSelectionProfile::Recommended;
    let recommended = selected_linter(&recommended).unwrap();
    assert!(
        !recommended
            .enabled_rule_ids()
            .iter()
            .any(|id| id.as_str() == "js:dynamic-code.eval")
    );

    let mut override_config = Config::default();
    override_config.cli.provider = Provider::Js;
    override_config.cli.profile = RuleSelectionProfile::Recommended;
    override_config.core.overrides = vec![
        glass_lint_core::RuleOverride::new(
            "js:dynamic-code.eval",
            glass_lint_core::RuleState::Enabled,
        )
        .unwrap(),
    ];
    let overridden = selected_linter(&override_config).unwrap();
    assert!(
        overridden
            .enabled_rule_ids()
            .iter()
            .any(|id| id.as_str() == "js:dynamic-code.eval")
    );
}

#[test]
fn validated_config_reuses_prepared_rule_selection() {
    let mut config = Config::default();
    config.cli.provider = Provider::Js;
    config.core.overrides = vec![
        glass_lint_core::RuleOverride::new(
            "js:dynamic-code.eval",
            glass_lint_core::RuleState::Enabled,
        )
        .unwrap(),
    ];

    let validated = config.validate().unwrap();
    let linter = selected_linter(&validated).unwrap();

    assert!(
        linter
            .enabled_rule_ids()
            .iter()
            .any(|id| id.as_str() == "js:dynamic-code.eval")
    );
}

#[test]
fn project_timeout_is_validated_at_the_cli_boundary() {
    let mut config = Config::default();
    config.cli.project.max_timeout_ms = 0;
    let error = config.validate().unwrap_err();
    assert!(error.to_string().contains("max_timeout_ms"));
}

#[test]
fn legacy_flat_project_limits_are_rejected() {
    let error =
        serde_json::from_str::<RawConfig>(r#"{"version":2,"cli":{"max_bytes":1024}}"#).unwrap_err();
    assert!(error.to_string().contains("unknown field"));
}
