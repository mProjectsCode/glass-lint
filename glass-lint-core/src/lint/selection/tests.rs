use super::*;

fn sel(selector: &str) -> RuleSelector {
    RuleSelector::parse(selector.to_owned()).unwrap()
}

#[test]
fn exact_selector_matches_identical_id() {
    let s = sel("js:network.request");
    assert!(s.matches("js:network.request"));
}

#[test]
fn exact_selector_rejects_longer_id() {
    let s = sel("js:network.request");
    assert!(!s.matches("js:network.request.extra"));
}

#[test]
fn exact_selector_rejects_shorter_id() {
    let s = sel("js:network.request");
    assert!(!s.matches("js:network"));
}

#[test]
fn exact_selector_rejects_different_id() {
    let s = sel("js:network.request");
    assert!(!s.matches("js:storage.cookie"));
}

#[test]
fn trailing_wildcard_matches_prefix() {
    let s = sel("js:network.*");
    assert!(s.matches("js:network.request"));
    assert!(s.matches("js:network.response"));
}

#[test]
fn trailing_wildcard_rejects_non_prefix() {
    let s = sel("js:network.*");
    assert!(!s.matches("js:storage.cookie"));
    assert!(!s.matches("other:network.request"));
}

#[test]
fn leading_wildcard_matches_suffix() {
    let s = sel("js:*.request");
    assert!(s.matches("js:network.request"));
    assert!(s.matches("js:storage.request"));
}

#[test]
fn leading_wildcard_rejects_non_suffix() {
    let s = sel("js:*.request");
    assert!(!s.matches("js:network.response"));
    assert!(!s.matches("js:storage.cookie"));
}

#[test]
fn wildcard_both_sides_matches_contains() {
    let s = sel("js:*.network.*");
    assert!(s.matches("js:prefix.network.suffix"));
    assert!(s.matches("js:x.network.y"));
}

#[test]
fn wildcard_both_sides_rejects_without_middle() {
    let s = sel("js:*.network.*");
    assert!(!s.matches("js:storage.request"));
    assert!(!s.matches("ts:prefix.network.suffix"));
}

#[test]
fn multiple_wildcards_match_complex_patterns() {
    let s = sel("js:*.*");
    assert!(s.matches("js:network.request"));
    assert!(s.matches("js:storage.cookie"));
    assert!(!s.matches("ts:network.request"));
}

#[test]
fn wildcard_any_provider_and_name_matches_all() {
    let s = sel("*:*");
    assert!(s.matches("js:network.request"));
    assert!(s.matches("anything:at.all"));
}

#[test]
fn empty_selector_is_rejected_at_parse() {
    assert!(RuleSelector::parse(String::new()).is_err());
}

#[test]
fn adjacent_wildcards_do_not_cause_false_negatives() {
    let s = sel("a:*");
    assert!(s.matches("a:x.b"));
    assert!(s.matches("a:b"));
    assert!(!s.matches("b:x.c"));
}

#[test]
fn wildcard_pattern_validates_its_own_rule_parts() {
    for selector in ["JS:*", "js:*:extra", "js:*..request", "js:-*request"] {
        assert!(
            RuleSelector::parse(selector.to_owned()).is_err(),
            "{selector}"
        );
    }
}

#[test]
fn wildcard_can_supply_a_valid_boundary_before_a_literal() {
    let s = sel("js:*-request");
    assert!(s.matches("js:network-request"));
    assert!(!s.matches("js:request"));
}
