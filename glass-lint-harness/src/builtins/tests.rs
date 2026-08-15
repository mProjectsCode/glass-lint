use super::*;

#[test]
fn explicit_rules_follow_target_catalog_composition() {
    let browser_rule = RuleId::parse("browser:browser.file-dialog").unwrap();
    assert!(linter_for_rules(BuiltinProvider::Obsidian, [browser_rule.clone()]).is_ok());
    assert!(linter_for_rules(BuiltinProvider::Js, [browser_rule]).is_err());
}
