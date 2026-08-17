use super::*;

#[test]
fn value_matcher_any_value_kind_is_any() {
    let m = ValueMatcher::any_value();
    assert_eq!(m.kind(), &ValueMatcherKind::Any);
}

#[test]
fn value_matcher_static_string_default_is_any() {
    let m = ValueMatcher::static_string();
    assert_eq!(
        m.kind(),
        &ValueMatcherKind::StaticString(StaticStringPredicate::new(StaticStringPredicateKind::Any))
    );
}

#[test]
fn value_matcher_equals_creates_exact_predicate() {
    let m = ValueMatcher::static_string().try_equals("hello").unwrap();
    assert_eq!(
        m.kind(),
        &ValueMatcherKind::StaticString(StaticStringPredicate::new(
            StaticStringPredicateKind::Exact(vec!["hello".into()])
        ))
    );
}

#[test]
fn value_matcher_equals_any_creates_multi_exact() {
    let m = ValueMatcher::static_string()
        .equals_any(["a", "b"])
        .unwrap();
    assert_eq!(
        m.kind(),
        &ValueMatcherKind::StaticString(StaticStringPredicate::new(
            StaticStringPredicateKind::Exact(vec!["a".into(), "b".into()])
        ))
    );
}

#[test]
fn value_matcher_equals_uses_canonical_static_values() {
    let exact = ValueMatcher::static_string().try_equals(" x ").unwrap();
    let alternatives = ValueMatcher::static_string().equals_any(["x"]).unwrap();
    assert_eq!(exact, alternatives);
}

#[test]
fn value_matcher_try_equals_rejects_empty_values() {
    assert_eq!(
        ValueMatcher::static_string().try_equals(" "),
        Err(QueryBuildError::EmptyStaticValue)
    );
    assert_eq!(
        ValueMatcher::static_string().try_equals(""),
        Err(QueryBuildError::EmptyStaticValue)
    );
}

#[test]
fn value_matcher_predicates_require_a_static_string_seed() {
    assert_eq!(
        ValueMatcher::any_value().try_equals("value"),
        Err(QueryBuildError::StaticStringMatcherRequired)
    );
}

#[test]
fn value_matcher_starts_with_any_creates_prefix_predicate() {
    let m = ValueMatcher::static_string()
        .starts_with_any(["https://"])
        .unwrap();
    assert_eq!(
        m.kind(),
        &ValueMatcherKind::StaticString(StaticStringPredicate::new(
            StaticStringPredicateKind::Prefix(vec!["https://".into()])
        ))
    );
}

#[test]
fn value_matcher_contains_any_creates_contains_any() {
    let m = ValueMatcher::static_string()
        .contains_any(["token", "secret"])
        .unwrap();
    assert_eq!(
        m.kind(),
        &ValueMatcherKind::StaticString(StaticStringPredicate::new(
            StaticStringPredicateKind::ContainsAny(vec!["secret".into(), "token".into()])
        ))
    );
}

#[test]
fn value_matcher_contains_all_creates_contains_all() {
    let m = ValueMatcher::static_string()
        .contains_all(["required", "field"])
        .unwrap();
    assert_eq!(
        m.kind(),
        &ValueMatcherKind::StaticString(StaticStringPredicate::new(
            StaticStringPredicateKind::ContainsAll(vec!["field".into(), "required".into()])
        ))
    );
}

#[test]
fn static_string_predicate_new_round_trips_kind() {
    let p = StaticStringPredicate::new(StaticStringPredicateKind::Any);
    assert!(matches!(p.kind, StaticStringPredicateKind::Any));
}

#[test]
fn argument_matcher_object_keys_holds_keys() {
    let m = ArgumentMatcher::object_keys(["x", "y"]).unwrap();
    assert!(matches!(m.kind(), ArgumentMatcherKind::ObjectKeys(keys) if keys == &["x", "y"]));
}

#[test]
fn argument_matcher_rooted_expressions_holds_chains() {
    let m = ArgumentMatcher::rooted_expressions(["document.body"]).unwrap();
    assert!(
        matches!(m.kind(), ArgumentMatcherKind::RootedExpressions(chains) if chains == &["document.body"])
    );
}

#[test]
fn argument_matcher_object_property_value_holds_property_and_matcher() {
    let value = ValueMatcher::static_string().try_equals("file").unwrap();
    let m = ArgumentMatcher::object_property_value("type", value).unwrap();
    assert!(
        matches!(m.kind(), ArgumentMatcherKind::ObjectPropertyValue { property, .. } if property == "type")
    );
}

#[test]
fn argument_matcher_from_value_matcher_converts() {
    let vm = ValueMatcher::any_value();
    let m: ArgumentMatcher = vm.into();
    assert!(matches!(m.kind(), ArgumentMatcherKind::Value(_)));
}

#[test]
fn argument_constraint_new_holds_index_and_matcher() {
    let m = ArgumentMatcher::object_keys(["k"]).unwrap();
    let c = ArgumentConstraint::new(ArgumentIndex::new_unchecked(2), m);
    assert_eq!(c.arg_index().get(), 2);
    assert!(matches!(
        c.predicate().kind(),
        ArgumentMatcherKind::ObjectKeys(_)
    ));
}

#[test]
fn object_key_collections_are_non_empty_canonical_and_bounded() {
    let matcher = ArgumentMatcher::object_keys(["method", "url", "url"]).unwrap();
    assert!(matches!(
        matcher.kind(),
        ArgumentMatcherKind::ObjectKeys(keys)
            if keys == &["method".to_string(), "url".to_string()]
    ));
    assert!(matches!(
        ArgumentMatcher::object_keys::<[&str; 0], &str>([]),
        Err(QueryBuildError::EmptyCollection(_))
    ));
    let keys: Vec<String> = (0..=limits::MAX_STATIC_ALTERNATIVES)
        .map(|index| format!("key{index}"))
        .collect();
    assert!(matches!(
        ArgumentMatcher::object_keys(keys),
        Err(QueryBuildError::CollectionTooLarge(_, _))
    ));
}

#[test]
fn rooted_expression_collections_validate_paths_and_limits() {
    let matcher = ArgumentMatcher::rooted_expressions(["document.body", "document.body"]).unwrap();
    assert!(matches!(
        matcher.kind(),
        ArgumentMatcherKind::RootedExpressions(paths)
            if paths == &["document.body".to_string()]
    ));
    assert!(matches!(
        ArgumentMatcher::rooted_expressions(["document..body"]),
        Err(QueryBuildError::MalformedChain(_))
    ));
    let paths: Vec<String> = (0..=limits::MAX_STATIC_ALTERNATIVES)
        .map(|index| format!("document.node{index}"))
        .collect();
    assert!(matches!(
        ArgumentMatcher::rooted_expressions(paths),
        Err(QueryBuildError::CollectionTooLarge(_, _))
    ));
}
