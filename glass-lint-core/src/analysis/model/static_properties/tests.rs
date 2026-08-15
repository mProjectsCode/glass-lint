use std::collections::BTreeMap;

use glass_lint_datastructures::NameTable;

use super::*;

fn intern(names: &mut NameTable, text: &str) -> NameId {
    names.intern(text).unwrap()
}

#[test]
fn duplicate_keys_keep_last_write_in_source_order() {
    let mut names = NameTable::default();
    let a = intern(&mut names, "a");
    let b = intern(&mut names, "b");
    let mut properties = StaticProperties::new();
    assert!(properties.insert(a, 1));
    assert!(properties.insert(b, 2));
    assert!(properties.insert(a, 3));
    assert_eq!(properties.get(a), Some(&3));
    assert_eq!(properties.get(b), Some(&2));
    assert_eq!(properties.len(), 2);
    assert!(!properties.is_empty());
    assert!(properties.contains_key(a));
    assert!(!properties.contains_key(intern(&mut names, "missing")));
    assert_eq!(properties.keys().collect::<Vec<_>>(), vec![a, b]);
    assert_eq!(
        properties.iter().collect::<Vec<_>>(),
        vec![(a, &3), (b, &2)]
    );
}

#[test]
fn bound_exhaustion_fails_new_insertion() {
    let mut names = NameTable::default();
    let mut properties = StaticProperties::new();
    for index in 0..MAX_OBJECT_KEYS {
        assert!(properties.insert(intern(&mut names, &format!("key_{index}")), index));
    }
    assert!(!properties.insert(intern(&mut names, "overflow"), 0));
    assert_eq!(properties.len(), MAX_OBJECT_KEYS);
}

#[test]
fn bound_exhaustion_never_rejects_existing_key_replacement() {
    let mut names = NameTable::default();
    let a = intern(&mut names, "a");
    let mut properties = StaticProperties::new();
    assert!(properties.insert(a, 0));
    for index in 1..MAX_OBJECT_KEYS {
        assert!(properties.insert(intern(&mut names, &format!("key_{index}")), index));
    }
    assert!(properties.insert(a, 1));
    assert_eq!(properties.get(a), Some(&1));
    assert_eq!(properties.len(), MAX_OBJECT_KEYS);
}

#[test]
fn to_const_object_projects_text_keys_with_unknown_values() {
    let mut names = NameTable::default();
    let mut properties = StaticProperties::new();
    properties.insert(intern(&mut names, "b"), 2);
    properties.insert(intern(&mut names, "a"), 1);
    let resolve = |key| names.resolve(key).map(SmolStr::new);
    assert_eq!(
        properties.to_const_object(&resolve),
        Some(ConstValue::Object(BTreeMap::from([
            ("a".into(), ConstValue::Unknown),
            ("b".into(), ConstValue::Unknown),
        ])))
    );
}

#[test]
fn unresolved_property_name_invalidates_object_projection() {
    let properties = StaticProperties::<()>::new();
    let resolve = |_key| None;
    assert_eq!(
        properties
            .to_const_object(&resolve)
            .unwrap_or(ConstValue::Unknown),
        ConstValue::Object(BTreeMap::new())
    );

    let mut names = NameTable::default();
    let unresolved = names.intern("missing").unwrap();
    let mut properties = StaticProperties::new();
    properties.insert(unresolved, ());
    assert_eq!(
        properties.to_const_object(&resolve),
        None,
        "unresolved property names must not be silently omitted"
    );
}
