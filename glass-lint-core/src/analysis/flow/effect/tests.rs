use glass_lint_datastructures::{NamePath, PathId, PathInterner, PathSegment};

use super::*;
use crate::analysis::{
    facts,
    facts::{FactId, FactPayload, FactStream, Frozen},
    value::{FunctionId, ValueId},
};

fn collect_effects(source: &str) -> (FactStream<Frozen>, FunctionEffects) {
    collect_effects_with_limit(source, usize::MAX)
}

#[test]
fn chain_owned_resolves_direct_call_with_rooted_or_syntactic_chain() {
    let (stream, _effects) = collect_effects("document.createElement('script');");
    let fact = stream
        .facts()
        .iter()
        .find(|f| matches!(&f.payload, FactPayload::Call { .. }))
        .expect("call fact should exist");
    let cref = CallEffectRef {
        stream: &stream,
        event: fact.id,
    };
    let names = stream.names();
    let chain = cref
        .chain_owned(names)
        .expect("direct call should have a chain");
    let chain: &NamePath = &chain;
    assert!(
        names
            .resolve_path(chain)
            .is_some_and(|s| s.eq_chain("document.createElement")),
        "chain should be document.createElement, got {}",
        names
            .resolve_path(chain)
            .map_or_else(|| "(unresolvable)".to_string(), |s| s.to_string())
    );
    assert!(cref.chain().is_some(), "borrowed chain should exist");
    assert!(cref.rooted(), "global member call should be rooted");
}

#[test]
fn chain_owned_falls_back_to_callee_name_for_alias_call() {
    let (stream, _effects) =
        collect_effects("function fetch(url) { return url; } const alias = fetch; alias('/api');");
    let names = stream.names();
    let call_facts: Vec<_> = stream
        .facts()
        .iter()
        .filter(|f| matches!(&f.payload, FactPayload::Call { .. }))
        .collect();
    assert!(!call_facts.is_empty(), "expected at least 1 call fact");
    let alias_call = call_facts[0];
    let cref = CallEffectRef {
        stream: &stream,
        event: alias_call.id,
    };
    let chain = cref
        .chain_owned(names)
        .expect("alias call should have a chain via callee_name fallback");
    let chain: &NamePath = &chain;
    assert!(
        names
            .resolve_path(chain)
            .is_some_and(|s| s.eq_chain("alias")),
        "alias call chain should resolve to the callee name 'alias', got {:?}",
        names.resolve_path(chain)
    );
}

#[test]
fn rooted_is_false_for_non_global_call() {
    let (stream, _effects) = collect_effects("function fn() { return 1; } fn();");
    let call_facts: Vec<_> = stream
        .facts()
        .iter()
        .filter(|f| matches!(&f.payload, FactPayload::Call { .. }))
        .collect();
    assert!(!call_facts.is_empty(), "expected at least 1 call fact");
    let call_fact = call_facts[0];
    let cref = CallEffectRef {
        stream: &stream,
        event: call_fact.id,
    };
    assert!(!cref.rooted(), "local function call should not be rooted");
}

#[test]
fn effective_args_unwraps_call_invocation() {
    let (stream, _effects) =
        collect_effects("function fetch(url) { return url; } fetch.call(null, '/api');");
    let call_facts: Vec<_> = stream
        .facts()
        .iter()
        .filter(|f| matches!(&f.payload, FactPayload::Call { .. }))
        .collect();
    assert!(!call_facts.is_empty(), "expected at least 1 call fact");
    let call_fact = call_facts[0];
    let cref = CallEffectRef {
        stream: &stream,
        event: call_fact.id,
    };
    let effective = cref
        .effective_args()
        .expect(".call() should have effective args");
    assert_eq!(
        effective.len(),
        1,
        ".call() drops receiver, expected 1 arg, got {}",
        effective.len()
    );
    let values = stream.values();
    let is_api = effective[0].base_value != ValueId::UNKNOWN
        && values
            .static_string(effective[0].base_value)
            .is_some_and(|s| s == "/api");
    assert!(is_api, "effective arg should be '/api'");
}

#[test]
fn effective_args_unwraps_apply_invocation() {
    let (stream, _effects) =
        collect_effects("function fetch(url) { return url; } fetch.apply(null, ['/api']);");
    let call_facts: Vec<_> = stream
        .facts()
        .iter()
        .filter(|f| matches!(&f.payload, FactPayload::Call { .. }))
        .collect();
    assert!(!call_facts.is_empty(), "expected at least 1 call fact");
    let call_fact = call_facts[0];
    let cref = CallEffectRef {
        stream: &stream,
        event: call_fact.id,
    };
    let effective = cref
        .effective_args()
        .expect(".apply() should have effective args");
    assert_eq!(
        effective.len(),
        1,
        ".apply() drops receiver and unwraps, expected 1 arg, got {}",
        effective.len()
    );
    let values = stream.values();
    let is_api = effective[0].base_value != ValueId::UNKNOWN
        && values
            .static_string(effective[0].base_value)
            .is_some_and(|s| s == "/api");
    assert!(is_api, "effective arg should be '/api'");
}

#[test]
fn call_fact_returns_none_for_unknown_id() {
    let (stream, _effects) = collect_effects("const x = 1;");
    let unknown = FactId(u32::MAX);
    let cref = CallEffectRef {
        stream: &stream,
        event: unknown,
    };
    assert!(cref.call_fact().is_none());
    assert!(cref.chain().is_none());
    assert!(!cref.rooted());
    assert_eq!(cref.result(), ValueId::UNKNOWN);
    assert!(cref.provenance().is_none());
    assert!(cref.target().is_none());
    assert!(cref.effective_args().is_none());
    let names = stream.names();
    assert!(cref.chain_owned(names).is_none());
}

#[test]
fn chain_returns_borrowed_without_callee_name_fallback() {
    let (stream, _effects) = collect_effects("document.createElement('script');");
    let fact = stream
        .facts()
        .iter()
        .find(|f| matches!(&f.payload, FactPayload::Call { .. }))
        .expect("call fact should exist");
    let cref = CallEffectRef {
        stream: &stream,
        event: fact.id,
    };
    let names = stream.names();
    let owned = cref.chain_owned(names).unwrap();
    let borrowed = cref.chain().unwrap();
    assert_eq!(&*owned, borrowed, "owned chain should match borrowed");
}

#[test]
fn call_argument_indexes_into_correct_call() {
    let (_stream, effects) = collect_effects(
        "function fn() { document.head.appendChild(document.createElement('script')); }",
    );
    let effect = effects
        .get(FunctionId(1))
        .expect("effect for fn should exist");
    let by_index = effect
        .call_argument(EffectCallId(0), 0)
        .expect("argument at index 0 should exist");
    assert_eq!(by_index.index(), 0);
}

#[test]
fn call_argument_returns_none_for_missing_index() {
    let (_stream, effects) =
        collect_effects("document.head.appendChild(document.createElement('script'));");
    let effect = effects
        .get(FunctionId(0))
        .expect("script effect should exist");
    assert!(effect.call_argument(EffectCallId(0), 999).is_none());
    assert!(effect.call_argument(EffectCallId(usize::MAX), 0).is_none());
}

#[test]
fn effects_budget_exhausted_with_limited_budget() {
    let (_stream, effects) =
        collect_effects_with_limit("function a() { return 1; } function b() { return a(); }", 2);
    assert!(effects.budget_exhausted());
}

#[test]
fn effects_operation_count_scales_with_program_size() {
    let (_stream, effects) = collect_effects("const x = 1; const y = x + 1;");
    let count = effects.operation_count();
    assert!(count > 0, "should consume budget for declarations");
}

#[test]
fn effects_budget_exhausted_false_with_unlimited_budget() {
    let (_stream, effects) = collect_effects("const x = 1;");
    assert!(!effects.budget_exhausted());
}

#[test]
fn collect_creates_program_level_function() {
    let (_stream, effects) = collect_effects("const x = 1;");
    assert!(effects.get(FunctionId(0)).is_some());
}

#[test]
fn collect_creates_user_defined_functions() {
    let (_stream, effects) = collect_effects("function f() { return 1; }");
    assert!(effects.get(FunctionId(1)).is_some());
}

#[test]
fn parameter_ref_index_and_is_root() {
    let mut paths = PathInterner::new();
    let non_empty = paths.append(PathId::EMPTY, PathSegment::Index(0)).unwrap();
    let params = [
        ParameterRef {
            index: 0,
            path: PathId::EMPTY,
        },
        ParameterRef {
            index: 1,
            path: non_empty,
        },
    ];
    assert_eq!(params[0].index(), 0);
    assert!(params[0].is_root());
    assert_eq!(params[1].index(), 1);
    assert!(!params[1].is_root());
}

#[test]
fn effect_call_id_is_newtype() {
    assert_ne!(EffectCallId(0), EffectCallId(1));
    assert_eq!(EffectCallId(5), EffectCallId(5));
}

fn collect_effects_with_limit(source: &str, limit: usize) -> (FactStream<Frozen>, FunctionEffects) {
    let stream = facts::build_test_facts(source, "test.js");
    let effects = FunctionEffects::collect(&stream, limit);
    (stream, effects)
}
