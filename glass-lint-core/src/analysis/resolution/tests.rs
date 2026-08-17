use std::collections::BTreeMap;

use glass_lint_datastructures::{NameTable, SymbolPath};
use swc_ecma_ast::Stmt;
use swc_ecma_visit::{Visit, VisitWith};

use super::*;
use crate::analysis::{
    SemanticBudget,
    model::value::{MAX_VALUES, StaticObject, Value},
    scope::ScopeGraph,
    semantic::SpanNormalizer,
    syntax::{UnknownReason, constant::MAX_ARRAY_ITEMS},
};

#[derive(Default)]
struct NamedIdentifiers {
    name: &'static str,
    matches: Vec<Ident>,
}

impl Visit for NamedIdentifiers {
    fn visit_ident(&mut self, ident: &Ident) {
        if ident.sym == self.name {
            self.matches.push(ident.clone());
        }
    }
}

fn last_named_identifier(program: &Program, name: &'static str) -> Ident {
    let mut identifiers = NamedIdentifiers {
        name,
        matches: Vec::new(),
    };
    program.visit_with(&mut identifiers);
    identifiers
        .matches
        .pop()
        .expect("test source should contain the named identifier")
}

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
        SymbolCallProvenance::Unknown(UnknownReason::BudgetExhausted { limit: MAX_VALUES })
    );
}

#[test]
fn provenance_identity_replacement_preserves_resolution_details() {
    let provenance = ResolutionProvenance::from_parts(
        Some(SymbolPath::from("source.root")),
        SymbolCallProvenance::Local,
        None,
        Some((
            SymbolPath::from("returned.source"),
            SymbolPath::from("returned.member"),
        )),
        None,
        Some(SymbolPath::from("source.alias")),
    );
    let finalized = provenance.with_call_identity(
        SymbolCallProvenance::Global {
            name: "fetch".into(),
        },
        Some(SymbolMemberProvenance::ModuleNamespace {
            module: "pkg".into(),
            member: "fetch".into(),
        }),
    );

    assert_eq!(
        finalized.rooted_chain,
        Some(SymbolPath::from("source.root"))
    );
    assert_eq!(
        finalized.returned_member,
        Some((
            SymbolPath::from("returned.source"),
            SymbolPath::from("returned.member"),
        ))
    );
    assert_eq!(
        finalized.syntactic_chain,
        Some(SymbolPath::from("source.alias"))
    );
    assert!(finalized.bound_arguments.is_none());
    assert!(matches!(
        finalized.call,
        SymbolCallProvenance::Global { ref name } if name == "fetch"
    ));
    assert!(matches!(
        finalized.module_member,
        Some(SymbolMemberProvenance::ModuleNamespace { ref module, ref member })
            if module == "pkg" && member == "fetch"
    ));
}

#[test]
fn joined_binding_constant_uses_the_retained_witness() {
    let source = "let value = 'first'; if (flag) value = 'second'; value;";
    let parsed = crate::parse_test_source(source, "joined-constant.js").unwrap();
    let ident = last_named_identifier(&parsed.program, "value");
    let budget = SemanticBudget::default();
    let resolver = Resolver::collect(&parsed.program, source, &budget);

    let constant = resolver.scope_graph().ident_value_seed(&ident).constant;
    assert_eq!(constant, ConstValue::String("first".into()));
}

#[test]
fn joined_object_member_does_not_cross_into_another_witness() {
    let source = "let value = { first: 'one' }; if (flag) value = { second: 'two' }; value.second;";
    let parsed = crate::parse_test_source(source, "joined-member.js").unwrap();
    let Program::Script(script) = &parsed.program else {
        panic!("test source should parse as a script");
    };
    let Stmt::Expr(statement) = script.body.last().unwrap() else {
        panic!("test source should end in an expression");
    };
    let Expr::Member(_member) = &*statement.expr else {
        panic!("test source should end in a member expression");
    };
    let budget = SemanticBudget::default();
    let resolver = Resolver::collect(&parsed.program, source, &budget);

    assert_eq!(
        crate::analysis::syntax::constant::evaluate(&*statement.expr, resolver.scope_graph()),
        ConstValue::Unknown
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
