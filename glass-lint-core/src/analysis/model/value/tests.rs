use super::*;

#[test]
fn invalid_value_ids_fail_closed() {
    let arena = ValueTable::default();
    assert!(arena.get(ValueId::from_test(u32::MAX)).is_none());
    assert!(arena.get(ValueId::UNKNOWN).is_some());
}

#[test]
fn invalid_binding_targets_fail_closed() {
    let mut table = ValueTable::default();
    let key = BindingKey::new(crate::analysis::model::scope::BindingRoot::Binding {
        function: FunctionId::from_test(0),
        binding: crate::analysis::model::scope::BindingId::from_test(0),
        version: crate::analysis::model::scope::BindingVersion::from_test(0),
    });

    let result = table.intern(Value::Binding {
        key,
        target: ValueId::from_test(u32::MAX),
    });

    assert_eq!(result, ValueId::UNKNOWN);
    assert!(table.exhausted());
    assert!(table.get(ValueId::from_test(1)).is_none());
}

#[test]
fn value_capacity_is_typed_as_exhaustion() {
    let mut table = ValueTable::default();
    for index in 0..MAX_VALUES {
        let _ = table.intern(Value::StaticNumber(index));
    }
    assert!(table.exhausted());
    assert_eq!(
        table.intern(Value::StaticNumber(MAX_VALUES + 1)),
        ValueId::UNKNOWN
    );
}

#[test]
fn callable_value_constructs_and_exposes_target() {
    let target = ValueId::from_test(42);
    let cv = CallableValue::new(target);
    assert_eq!(cv.target(), target);
}

#[test]
fn intern_with_binding_wraps_in_binding_when_key_provided() {
    let mut table = ValueTable::default();
    let inner = table.intern(Value::StaticString("hello".into()));
    let key = BindingKey::new(crate::analysis::model::scope::BindingRoot::Binding {
        function: FunctionId::from_test(0),
        binding: crate::analysis::model::scope::BindingId::from_test(1),
        version: crate::analysis::model::scope::BindingVersion::from_test(0),
    });
    let wrapped = table.intern_with_binding(Value::StaticString("hello".into()), Some(key));
    assert_ne!(wrapped, inner);
    assert!(matches!(table.get(wrapped), Some(Value::Binding { .. })));
}

#[test]
fn intern_with_binding_returns_direct_id_when_no_binding() {
    let mut table = ValueTable::default();
    let id = table.intern_with_binding(Value::StaticNumber(99), None);
    assert!(matches!(table.get(id), Some(Value::StaticNumber(99))));
}

#[test]
fn resolve_follows_binding_chain_to_terminal_value() {
    let mut table = ValueTable::default();
    let terminal = table.intern(Value::StaticString("target".into()));
    let key = BindingKey::new(crate::analysis::model::scope::BindingRoot::Binding {
        function: FunctionId::from_test(0),
        binding: crate::analysis::model::scope::BindingId::from_test(0),
        version: crate::analysis::model::scope::BindingVersion::from_test(0),
    });
    let binding = table.intern(Value::Binding {
        key,
        target: terminal,
    });
    let resolved = table.resolve(binding);
    assert_eq!(resolved, Some(&Value::StaticString("target".into())));
}

#[test]
fn resolve_follows_long_chain() {
    let mut table = ValueTable::default();
    let terminal = table.intern(Value::StaticString("target".into()));
    let mut prev = terminal;
    for i in 1..=20 {
        let key = BindingKey::new(crate::analysis::model::scope::BindingRoot::Binding {
            function: FunctionId::from_test(0),
            binding: crate::analysis::model::scope::BindingId::from_test(i),
            version: crate::analysis::model::scope::BindingVersion::from_test(0),
        });
        prev = table.intern(Value::Binding { key, target: prev });
    }
    assert_eq!(
        table.resolve(prev),
        Some(&Value::StaticString("target".into()))
    );
}

#[test]
fn resolve_returns_terminal_for_non_binding_value() {
    let mut table = ValueTable::default();
    let id = table.intern(Value::StaticString("direct".into()));
    assert_eq!(
        table.resolve(id),
        Some(&Value::StaticString("direct".into()))
    );
}

#[test]
fn resolve_returns_none_for_unknown_id() {
    let table = ValueTable::default();
    assert!(table.resolve(ValueId::from_test(u32::MAX)).is_none());
}

#[test]
fn binding_slot_returns_named_slot_for_binding_values() {
    let mut table = ValueTable::default();
    let target = table.intern(Value::StaticString("target".into()));
    let key = BindingKey::new(crate::analysis::model::scope::BindingRoot::Binding {
        function: FunctionId::from_test(0),
        binding: crate::analysis::model::scope::BindingId::from_test(0),
        version: crate::analysis::model::scope::BindingVersion::from_test(0),
    });
    let binding = table.intern(Value::Binding { key, target });
    assert_eq!(
        table.binding_slot(binding),
        Some(BindingSlot::new(
            FunctionId::from_test(0),
            crate::analysis::model::scope::BindingId::from_test(0),
            NamePath::new(),
        ))
    );
    assert!(table.binding_slot(target).is_none());
}

#[test]
fn static_string_returns_string_for_static_string_value() {
    let mut table = ValueTable::default();
    let id = table.intern(Value::StaticString("extracted".into()));
    assert_eq!(table.static_string(id), Some("extracted"));
}

#[test]
fn static_string_returns_none_for_non_string_value() {
    let mut table = ValueTable::default();
    let id = table.intern(Value::StaticNumber(42));
    assert!(table.static_string(id).is_none());
}

#[test]
fn static_string_follows_binding_chain() {
    let mut table = ValueTable::default();
    let target = table.intern(Value::StaticString("chained".into()));
    let key = BindingKey::new(crate::analysis::model::scope::BindingRoot::Binding {
        function: FunctionId::from_test(0),
        binding: crate::analysis::model::scope::BindingId::from_test(0),
        version: crate::analysis::model::scope::BindingVersion::from_test(0),
    });
    let binding = table.intern(Value::Binding { key, target });
    assert_eq!(table.static_string(binding), Some("chained"));
}

#[test]
fn intern_static_object_creates_object_with_canonical_names() {
    let mut table = ValueTable::default();
    let mut names = NameTable::default();
    let key_a = names.intern("a").unwrap();
    let key_b = names.intern("b").unwrap();
    let val_a = table.intern(Value::StaticString("val_a".into()));
    let val_b = table.intern(Value::StaticNumber(1));
    let pairs = vec![("b".into(), val_b), ("a".into(), val_a)];
    let obj = table.intern_static_object(pairs, &names, None);
    let value = table.get(obj).expect("object should exist");
    let Value::StaticObject(object) = value else {
        panic!("expected StaticObject, got {value:?}");
    };
    assert_eq!(object.len(), 2);
    assert!(object.iter().any(|(k, _)| k == key_a));
    assert!(object.iter().any(|(k, _)| k == key_b));
}

#[test]
fn intern_static_object_exhausts_on_unknown_name() {
    let mut table = ValueTable::default();
    let names = NameTable::default();
    let val = table.intern(Value::StaticNumber(0));
    let pairs = vec![("unknown".into(), val)];
    let result = table.intern_static_object(pairs, &names, None);
    assert_eq!(result, ValueId::UNKNOWN);
    assert!(table.exhausted());
}

#[test]
fn allocate_object_id_returns_increasing_ids() {
    let mut table = ValueTable::default();
    let a = table.allocate_object_id().expect("first id");
    let b = table.allocate_object_id().expect("second id");
    assert_eq!(ResolvedObjectId::from_test(0), a);
    assert_eq!(ResolvedObjectId::from_test(1), b);
}

#[test]
fn allocate_object_id_exhausts_at_max() {
    let mut table = ValueTable::default();
    for _ in 0..65_536 {
        table.allocate_object_id();
    }
    assert!(table.allocate_object_id().is_none());
    assert!(table.exhausted());
}

#[test]
fn value_id_unknown_is_zero() {
    assert_eq!(ValueId::UNKNOWN, ValueId::from_test(0));
}

#[test]
fn static_object_looks_up_properties_and_iterates_stably() {
    let mut names = NameTable::default();
    let key_a = names.intern("a").unwrap();
    let key_b = names.intern("b").unwrap();
    let mut table = ValueTable::default();
    let val_a = table.intern(Value::StaticString("a".into()));
    let val_b = table.intern(Value::StaticString("b".into()));
    let object =
        StaticObject::new(vec![(key_a, val_a), (key_b, val_b)]).expect("object fits budget");

    assert_eq!(object.len(), 2);
    assert!(!object.is_empty());
    assert_eq!(object.get(key_a), Some(val_a));
    assert_eq!(object.get(key_b), Some(val_b));
    assert_eq!(object.get(names.intern("missing").unwrap()), None);
    assert!(object.contains_key(key_a));
    assert!(!object.contains_key(names.intern("missing").unwrap()));

    let pairs: Vec<_> = object.iter().collect();
    assert_eq!(pairs, vec![(key_a, val_a), (key_b, val_b)]);
}

#[test]
fn static_object_path_traversal_follows_properties_only() {
    let mut names = NameTable::default();
    let key_a = names.intern("a").unwrap();
    let key_b = names.intern("b").unwrap();
    let mut table = ValueTable::default();
    let val_a = table.intern(Value::StaticString("a".into()));
    let val_b = table.intern(Value::StaticString("b".into()));
    let object =
        StaticObject::new(vec![(key_a, val_a), (key_b, val_b)]).expect("object fits budget");

    assert_eq!(
        object.value_at_segment(PathSegment::Property(key_a)),
        Some(val_a)
    );
    assert_eq!(
        object.value_at_segment(PathSegment::Property(key_b)),
        Some(val_b)
    );
    assert_eq!(
        object.value_at_segment(PathSegment::Property(names.intern("missing").unwrap())),
        None
    );
    assert_eq!(object.value_at_segment(PathSegment::Index(0)), None);
}

#[test]
fn static_object_new_rejects_over_budget_shapes() {
    let mut names = NameTable::default();
    let mut table = ValueTable::default();
    let value = table.intern(Value::StaticString("v".into()));
    let mut entries = Vec::with_capacity(257);
    for index in 0..257 {
        entries.push((names.intern(&format!("key_{index}")).unwrap(), value));
    }
    assert!(
        StaticObject::new(entries).is_none(),
        "an object with more properties than the bound must be unknown"
    );
}

#[test]
fn static_object_new_applies_last_write_wins_to_duplicates() {
    let mut names = NameTable::default();
    let key = names.intern("key").unwrap();
    let mut table = ValueTable::default();
    let first = table.intern(Value::StaticString("first".into()));
    let last = table.intern(Value::StaticString("last".into()));
    let object = StaticObject::new(vec![(key, first), (key, last)]).expect("object fits budget");
    assert_eq!(object.len(), 1);
    assert_eq!(object.get(key), Some(last));
}

#[test]
fn value_debug_and_partial_eq() {
    let v1 = Value::StaticString("a".into());
    let v2 = Value::StaticString("a".into());
    let v3 = Value::StaticString("b".into());
    assert_eq!(v1, v2);
    assert_ne!(v1, v3);
}
