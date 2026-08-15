use glass_lint_datastructures::NameTable;
use swc_common::{BytePos, Span};

use super::*;

#[test]
fn binding_versions_are_part_of_identity() {
    let mut first = BindingKey::new(BindingRoot::Binding {
        function: FunctionId::from_test(1),
        binding: BindingId::from_test(2),
        version: BindingVersion::from_test(0),
    });
    let mut names = NameTable::default();
    let value = names.intern("value").unwrap();
    first.append_segment(value);
    let mut second = BindingKey::new(BindingRoot::Binding {
        function: FunctionId::from_test(1),
        binding: BindingId::from_test(2),
        version: BindingVersion::from_test(1),
    });
    second.append_segment(value);
    assert_ne!(first, second);
}

#[test]
fn scope_id_index_and_from_usize() {
    let id = ScopeId::from_test(5);
    assert_eq!(id.index_for_test(), 5);
}

#[test]
fn scoped_name_round_trips_scope_and_name() {
    let mut names = NameTable::default();
    let nid = names.intern("foo").unwrap();
    let sn = ScopedName::new(ScopeId::from_test(3), nid);
    assert_eq!(sn.scope(), ScopeId::from_test(3));
    assert_eq!(sn.name(), nid);
}

#[test]
fn binding_root_global_variant() {
    let a = BindingRoot::Global("window".into());
    let b = BindingRoot::Global("window".into());
    let c = BindingRoot::Global("document".into());
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn binding_root_binding_variants_differ_on_version() {
    let a = BindingRoot::Binding {
        function: FunctionId::from_test(1),
        binding: BindingId::from_test(2),
        version: BindingVersion::from_test(0),
    };
    let b = BindingRoot::Binding {
        function: FunctionId::from_test(1),
        binding: BindingId::from_test(2),
        version: BindingVersion::from_test(1),
    };
    assert_ne!(a, b);
}

#[test]
fn binding_slot_stays_constant_across_versions() {
    let mut first = BindingKey::new(BindingRoot::Binding {
        function: FunctionId::from_test(1),
        binding: BindingId::from_test(2),
        version: BindingVersion::from_test(0),
    });
    let mut second = BindingKey::new(BindingRoot::Binding {
        function: FunctionId::from_test(1),
        binding: BindingId::from_test(2),
        version: BindingVersion::from_test(1),
    });
    let mut names = NameTable::default();
    let value = names.intern("value").unwrap();
    first.append_segment(value);
    second.append_segment(value);
    assert_eq!(first.binding_slot(), second.binding_slot());
}

#[test]
fn binding_slot_round_trips_construction_from_components() {
    let mut names = NameTable::default();
    let path = NamePath::from_ids([names.intern("a").unwrap()]);
    let slot = BindingSlot::new(FunctionId::from_test(1), BindingId::from_test(2), path);
    let mut key = BindingKey::new(BindingRoot::Binding {
        function: FunctionId::from_test(1),
        binding: BindingId::from_test(2),
        version: BindingVersion::from_test(0),
    });
    key.append_segment(names.intern("a").unwrap());
    assert_eq!(key.binding_slot(), Some(slot));
}

#[test]
fn global_binding_keys_have_no_slot() {
    let key = BindingKey::new(BindingRoot::Global("window".into()));
    assert!(key.binding_slot().is_none());
}

#[test]
fn binding_key_new_creates_empty_path() {
    let key = BindingKey::new(BindingRoot::Global("g".into()));
    assert_eq!(
        key,
        BindingKey {
            root: BindingRoot::Global("g".into()),
            path: NamePath::new(),
        }
    );
}

#[test]
fn scope_kind_variants_are_distinct() {
    assert_ne!(ScopeKind::Program, ScopeKind::Function);
    assert_ne!(ScopeKind::Function, ScopeKind::Block);
    assert_ne!(ScopeKind::Block, ScopeKind::Dynamic);
    assert_eq!(ScopeKind::Program, ScopeKind::Program);
}

#[test]
fn scope_effect_dynamic_evaluation_span() {
    let span = Span::new(BytePos(10), BytePos(20));
    let effect = ScopeEffect::DynamicEvaluation { span };
    assert_eq!(effect.span(), span);
}

#[test]
fn binding_provenance_variants() {
    let local = BindingProvenance::Local;
    let alias = BindingProvenance::ValueAlias {
        target: NamePath::new(),
    };
    let bound_callable = BindingProvenance::BoundCallable {
        target: NamePath::new(),
        bound_arguments: vec![Some(BoundArgument::StaticString("x".into()))],
    };
    let module_ns = BindingProvenance::ModuleNamespace {
        module: "pkg".into(),
    };
    let static_string = BindingProvenance::StaticString("hello".into());
    assert_eq!(local, BindingProvenance::Local);
    assert_ne!(local, alias);
    assert_ne!(alias, bound_callable);
    assert_ne!(bound_callable, module_ns);
    assert_ne!(module_ns, static_string);
}

#[test]
fn bound_argument_static_string_and_rooted_expression() {
    let s = BoundArgument::StaticString("exact".into());
    let r = BoundArgument::RootedExpression(NamePath::new());
    assert_ne!(s, r);
    assert_eq!(s, BoundArgument::StaticString("exact".into()));
}

#[test]
fn function_id_converts_to_u32() {
    let id = FunctionId::from_test(42);
    let raw: u32 = id.into();
    assert_eq!(raw, 42);
}

#[test]
fn binding_id_and_version_are_newtypes() {
    assert_ne!(BindingId::from_test(1), BindingId::from_test(2));
    assert_ne!(BindingVersion::from_test(0), BindingVersion::from_test(1));
    assert_eq!(BindingId::from_test(5), BindingId::from_test(5));
}

#[test]
fn provenance_alternatives_overflow_is_exhausted_and_unknown() {
    let alias = BindingProvenance::ValueAlias {
        target: NamePath::new(),
    };
    let mut set = ProvenanceJoin::new(1);
    set.add(&ProvenanceAlternatives::single(BindingProvenance::Local));
    set.add(&ProvenanceAlternatives::single(alias));
    let set = set.alternatives();
    assert!(set.is_exhausted());
    assert!(set.is_unknown());
    assert_eq!(
        set.complete_witnesses().collect::<Vec<_>>(),
        vec![&BindingProvenance::Local]
    );
}

#[test]
fn provenance_alternatives_dedup_within_the_join_bound() {
    let alias = BindingProvenance::ValueAlias {
        target: NamePath::new(),
    };
    let mut set = ProvenanceJoin::new(4);
    set.add(&ProvenanceAlternatives::single(alias.clone()));
    set.add(&ProvenanceAlternatives::single(alias.clone()));
    set.add(&ProvenanceAlternatives::single(BindingProvenance::Local));
    let set = set.alternatives();
    assert!(set.is_joined());
    assert!(!set.is_exhausted());
    assert!(!set.is_unknown());
    assert_eq!(
        set.complete_witnesses().collect::<Vec<_>>(),
        vec![&alias, &BindingProvenance::Local]
    );
}

#[test]
fn provenance_alternatives_duplicate_at_bound_remains_complete() {
    let alias = BindingProvenance::ValueAlias {
        target: NamePath::new(),
    };
    let mut set = ProvenanceJoin::new(1);
    set.add(&ProvenanceAlternatives::single(alias.clone()));
    set.add(&ProvenanceAlternatives::single(alias.clone()));
    let set = set.alternatives();
    assert!(!set.is_exhausted());
    assert!(!set.is_unknown());
    assert_eq!(set.complete_witnesses().collect::<Vec<_>>(), vec![&alias]);
}

#[test]
fn unknown_only_alternatives_have_no_complete_witness() {
    let unknown = ProvenanceAlternatives::unknown();
    assert!(!unknown.has_complete_witness());
    assert_eq!(unknown.complete_witnesses().count(), 0);
    assert_eq!(unknown.preferred_witness(), None);
}

#[test]
fn preferred_witness_prefers_non_local_after_join() {
    let single = ProvenanceAlternatives::single(BindingProvenance::Local);
    assert_eq!(single.preferred_witness(), Some(&BindingProvenance::Local));

    let alias = BindingProvenance::ValueAlias {
        target: NamePath::new(),
    };
    let mut joined = ProvenanceJoin::new(4);
    joined.add(&ProvenanceAlternatives::single(BindingProvenance::Local));
    joined.add(&ProvenanceAlternatives::single(alias.clone()));
    let joined = joined.alternatives();
    assert_eq!(joined.preferred_witness(), Some(&alias));

    let mut local_only = ProvenanceJoin::new(4);
    local_only.add(&ProvenanceAlternatives::single(BindingProvenance::Local));
    let local_only = local_only.alternatives();
    assert_eq!(local_only.preferred_witness(), None);
}

#[test]
fn alias_assignment_constructors_own_the_alternative_set() {
    let mut names = NameTable::default();
    let name = names.intern("value").unwrap();
    let scope = ScopeId::from_test(1);
    let span = Span::new(BytePos(0), BytePos(1));

    let precise = AliasAssignment::from_alternatives(
        span,
        scope,
        name,
        BindingVersion::from_test(1),
        ProvenanceAlternatives::single(BindingProvenance::Local),
    );
    assert!(!precise.is_joined());
    assert_eq!(precise.preferred_witness(), Some(&BindingProvenance::Local));
    assert_eq!(precise.complete_witnesses().count(), 1);

    let mut exhausted = ProvenanceJoin::new(0);
    exhausted.add(&ProvenanceAlternatives::single(BindingProvenance::Local));
    assert!(exhausted.alternatives().is_exhausted());
    let joined = AliasAssignment::from_alternatives(
        span,
        scope,
        name,
        BindingVersion::from_test(2),
        exhausted.alternatives().clone(),
    );
    assert!(joined.is_joined());
    assert!(joined.alternatives().is_exhausted());
    assert_eq!(joined.complete_witnesses().count(), 0);
}
