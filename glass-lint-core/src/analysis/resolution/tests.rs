use std::collections::BTreeMap;

use glass_lint_datastructures::NameTable;

use super::*;
use crate::analysis::{
    SemanticBudget,
    model::value::{MAX_VALUES, StaticObject, Value},
    scope::ScopeGraph,
    semantic::SpanNormalizer,
    syntax::{BudgetComponent, UnknownReason, constant::MAX_ARRAY_ITEMS},
};

#[test]
fn unknown_value_keeps_unsupported_and_exhausted_distinct() {
    let names = NameTable::default();
    let scopes = ScopeGraph::create_for_test(names).freeze();
    let budget = SemanticBudget::default();
    let resolver = Resolver::new_for_test(scopes, SpanNormalizer::default(), &budget);
    assert_eq!(
        resolver.call_provenance_for_value(ValueId::UNKNOWN),
        SymbolCallProvenance::Unknown(UnknownReason::Unsupported)
    );

    let mut values = ValueTable::default();
    for value in 0..MAX_VALUES {
        let _ = values.intern(Value::StaticNumber(value));
    }
    assert!(values.exhausted());
    let names = NameTable::default();
    let scopes = ScopeGraph::create_for_test(names).freeze();
    let resolver_names = scopes.name_snapshot();
    let budget = SemanticBudget::default();
    let resolver = Resolver {
        scopes,
        names: resolver_names,
        coordinates: SpanNormalizer::default(),
        values,
        cache: ResolverCache::default(),
        budget: &budget,
    };
    assert_eq!(
        resolver.call_provenance_for_value(ValueId::UNKNOWN),
        SymbolCallProvenance::Unknown(UnknownReason::BudgetExhausted {
            component: BudgetComponent::Values,
            limit: MAX_VALUES,
            observed: None,
        })
    );
}

#[test]
fn const_value_follows_binding_chain_to_static_values() {
    let names = NameTable::default();
    let scopes = ScopeGraph::create_for_test(names).freeze();
    let budget = SemanticBudget::default();
    let mut resolver = Resolver::new_for_test(scopes, SpanNormalizer::default(), &budget);

    let inner = resolver.values.intern(Value::StaticString("hello".into()));
    let key = crate::analysis::model::scope::BindingKey::new(
        crate::analysis::model::scope::BindingRoot::Global("test".into()),
    );
    let id = resolver
        .values
        .intern(Value::Binding { key, target: inner });

    let result = resolver.const_value(id);
    assert_eq!(result, ConstValue::String("hello".into()));
}

#[test]
fn const_value_materializes_static_arrays_with_nested_bindings() {
    let names = NameTable::default();
    let scopes = ScopeGraph::create_for_test(names).freeze();
    let budget = SemanticBudget::default();
    let mut resolver = Resolver::new_for_test(scopes, SpanNormalizer::default(), &budget);

    let one = resolver.values.intern(Value::StaticNumber(1));
    let key = crate::analysis::model::scope::BindingKey::new(
        crate::analysis::model::scope::BindingRoot::Global("x".into()),
    );
    let wrapped = resolver.values.intern(Value::Binding { key, target: one });
    let two = resolver.values.intern(Value::StaticNumber(2));
    let array = resolver
        .values
        .intern(Value::StaticArray(vec![wrapped, two]));

    let result = resolver.const_value(array);
    assert_eq!(
        result,
        ConstValue::Array(vec![
            ConstValue::NonNegativeInteger(1),
            ConstValue::NonNegativeInteger(2),
        ])
    );
}

#[test]
fn intern_const_value_preserves_an_admitted_nested_shape() {
    let mut names = NameTable::default();
    names.intern("items").unwrap();
    let scopes = ScopeGraph::create_for_test(names).freeze();
    let budget = SemanticBudget::default();
    let mut resolver = Resolver::new_for_test(scopes, SpanNormalizer::default(), &budget);

    let mut nested = BTreeMap::new();
    nested.insert(
        "items".into(),
        ConstValue::array(vec![ConstValue::String("open".into())]),
    );
    let value = ConstValue::object(nested);
    let id = resolver.intern_const_value(value.clone(), None);

    assert_eq!(resolver.const_value(id), value);
}

#[test]
fn const_value_returns_unknown_for_uninterned_id() {
    let names = NameTable::default();
    let scopes = ScopeGraph::create_for_test(names).freeze();
    let budget = SemanticBudget::default();
    let resolver = Resolver::new_for_test(scopes, SpanNormalizer::default(), &budget);

    let result = resolver.const_value(ValueId::from_test(u32::MAX));
    assert_eq!(result, ConstValue::Unknown);
}

#[test]
fn const_value_materializes_static_object_with_mixed_values() {
    let mut names = NameTable::default();
    let key_num = names.intern("num").unwrap();
    let key_str = names.intern("str").unwrap();
    let key_arr = names.intern("arr").unwrap();
    let scopes = ScopeGraph::create_for_test(names).freeze();
    let budget = SemanticBudget::default();
    let mut resolver = Resolver::new_for_test(scopes, SpanNormalizer::default(), &budget);

    let num_id = resolver.values.intern(Value::StaticNumber(42));
    let str_id = resolver.values.intern(Value::StaticString("val".into()));
    let inner_arr = resolver.values.intern(Value::StaticArray(vec![num_id]));

    let obj_id = resolver.values.intern(Value::StaticObject(
        StaticObject::new(vec![
            (key_num, num_id),
            (key_str, str_id),
            (key_arr, inner_arr),
        ])
        .expect("object fits the static property budget"),
    ));

    let result = resolver.const_value(obj_id);
    assert_eq!(
        result,
        ConstValue::Object(BTreeMap::from([
            (
                "arr".into(),
                ConstValue::Array(vec![ConstValue::NonNegativeInteger(42)])
            ),
            ("num".into(), ConstValue::NonNegativeInteger(42)),
            ("str".into(), ConstValue::String("val".into())),
        ]))
    );
}

#[test]
fn const_value_returns_unknown_for_unknown_name_in_object() {
    let mut names_with = NameTable::default();
    let key = names_with.intern("key").unwrap();
    let names_empty = NameTable::default();
    let scopes = ScopeGraph::create_for_test(names_empty).freeze();
    let budget = SemanticBudget::default();
    let mut resolver = Resolver::new_for_test(scopes, SpanNormalizer::default(), &budget);

    let val_id = resolver.values.intern(Value::StaticString("v".into()));
    let obj_id = resolver.values.intern(Value::StaticObject(
        StaticObject::new(vec![(key, val_id)]).expect("object fits the static property budget"),
    ));

    let result = resolver.const_value(obj_id);
    assert_eq!(result, ConstValue::Unknown);
}

#[test]
fn const_value_returns_unknown_for_deeply_nested_structure() {
    let names = NameTable::default();
    let scopes = ScopeGraph::create_for_test(names).freeze();
    let budget = SemanticBudget::default();
    let mut resolver = Resolver::new_for_test(scopes, SpanNormalizer::default(), &budget);

    let leaf = resolver.values.intern(Value::StaticNumber(0));
    let mut current = leaf;
    for _ in 0..31 {
        current = resolver.values.intern(Value::StaticArray(vec![current]));
    }
    let result = resolver.const_value(current);
    assert!(
        matches!(result, ConstValue::Array(_)),
        "31 nesting levels should succeed"
    );

    current = resolver.values.intern(Value::StaticArray(vec![current]));
    let result = resolver.const_value(current);
    let mut inner = &result;
    loop {
        match inner {
            ConstValue::Array(elements) if elements.len() == 1 => inner = &elements[0],
            _ => break,
        }
    }
    assert_eq!(inner, &ConstValue::Unknown);
}

#[test]
fn const_value_materializes_large_flat_array() {
    let names = NameTable::default();
    let scopes = ScopeGraph::create_for_test(names).freeze();
    let budget = SemanticBudget::default();
    let mut resolver = Resolver::new_for_test(scopes, SpanNormalizer::default(), &budget);

    let ids: Vec<_> = (0..100)
        .map(|i| resolver.values.intern(Value::StaticNumber(i)))
        .collect();
    let array_id = resolver.values.intern(Value::StaticArray(ids));

    let result = resolver.const_value(array_id);
    assert_eq!(
        result,
        ConstValue::Array(
            (0..100)
                .map(ConstValue::NonNegativeInteger)
                .collect::<Vec<_>>()
        )
    );
}

#[test]
fn const_value_applies_the_shared_container_bound() {
    let names = NameTable::default();
    let scopes = ScopeGraph::create_for_test(names).freeze();
    let budget = SemanticBudget::default();
    let mut resolver = Resolver::new_for_test(scopes, SpanNormalizer::default(), &budget);

    let ids: Vec<_> = (0..=MAX_ARRAY_ITEMS)
        .map(|i| resolver.values.intern(Value::StaticNumber(i)))
        .collect();
    let array_id = resolver.values.intern(Value::StaticArray(ids));

    assert_eq!(resolver.const_value(array_id), ConstValue::Unknown);
}

#[test]
fn const_value_follows_binding_chain_through_reassignment() {
    let names = NameTable::default();
    let scopes = ScopeGraph::create_for_test(names).freeze();
    let budget = SemanticBudget::default();
    let mut resolver = Resolver::new_for_test(scopes, SpanNormalizer::default(), &budget);

    let inner = resolver.values.intern(Value::StaticString("first".into()));
    let key1 = crate::analysis::model::scope::BindingKey::new(
        crate::analysis::model::scope::BindingRoot::Global("v1".into()),
    );
    let first = resolver.values.intern(Value::Binding {
        key: key1,
        target: inner,
    });

    let key2 = crate::analysis::model::scope::BindingKey::new(
        crate::analysis::model::scope::BindingRoot::Global("v2".into()),
    );
    let second = resolver.values.intern(Value::Binding {
        key: key2,
        target: first,
    });

    assert_eq!(
        resolver.const_value(first),
        ConstValue::String("first".into())
    );
    assert_eq!(
        resolver.const_value(second),
        ConstValue::String("first".into())
    );
}

#[test]
fn call_provenance_follows_binding_to_global() {
    let names = NameTable::default();
    let scopes = ScopeGraph::create_for_test(names).freeze();
    let budget = SemanticBudget::default();
    let mut resolver = Resolver::new_for_test(scopes, SpanNormalizer::default(), &budget);

    let inner = resolver.values.intern(Value::Global("fetch".into()));
    let key = crate::analysis::model::scope::BindingKey::new(
        crate::analysis::model::scope::BindingRoot::Global("test".into()),
    );
    let id = resolver
        .values
        .intern(Value::Binding { key, target: inner });

    assert_eq!(
        resolver.call_provenance_for_value(id),
        SymbolCallProvenance::Global {
            name: "fetch".into()
        }
    );
}

#[test]
fn call_provenance_follows_multi_level_binding_chain() {
    let names = NameTable::default();
    let scopes = ScopeGraph::create_for_test(names).freeze();
    let budget = SemanticBudget::default();
    let mut resolver = Resolver::new_for_test(scopes, SpanNormalizer::default(), &budget);

    let inner = resolver.values.intern(Value::ModuleExport {
        module: "mod".into(),
        export: "fn".into(),
    });
    let key1 = crate::analysis::model::scope::BindingKey::new(
        crate::analysis::model::scope::BindingRoot::Global("a".into()),
    );
    let mid = resolver.values.intern(Value::Binding {
        key: key1,
        target: inner,
    });
    let key2 = crate::analysis::model::scope::BindingKey::new(
        crate::analysis::model::scope::BindingRoot::Global("b".into()),
    );
    let id = resolver.values.intern(Value::Binding {
        key: key2,
        target: mid,
    });

    assert_eq!(
        resolver.call_provenance_for_value(id),
        SymbolCallProvenance::ModuleExport {
            module: "mod".into(),
            export: "fn".into()
        }
    );
}

#[test]
fn value_exhaustion_distinguishes_unsupported_from_budget() {
    let names = NameTable::default();
    let scopes = ScopeGraph::create_for_test(names).freeze();
    let budget = SemanticBudget::default();
    let resolver = Resolver::new_for_test(scopes, SpanNormalizer::default(), &budget);
    assert!(!resolver.value_arena_exhausted());

    let mut values = ValueTable::default();
    for value in 0..MAX_VALUES {
        let _ = values.intern(Value::StaticNumber(value));
    }
    assert!(values.exhausted());
    let names = NameTable::default();
    let scopes = ScopeGraph::create_for_test(names).freeze();
    let resolver_names = scopes.name_snapshot();
    let budget = SemanticBudget::default();
    let resolver = Resolver {
        scopes,
        names: resolver_names,
        coordinates: SpanNormalizer::default(),
        values,
        cache: ResolverCache::default(),
        budget: &budget,
    };
    assert!(resolver.value_arena_exhausted());
}
