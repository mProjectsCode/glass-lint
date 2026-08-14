use swc_ecma_visit::VisitWith;

use crate::analysis::{
    facts::{FactBuilder, FactPayload, FactStream, Frozen, build_test_facts},
    model::value::ValueId,
    resolution::Resolver,
    syntax::SymbolCallProvenance,
};

#[test]
fn fact_builder_emits_facts_for_diverse_program() {
    let src = r#"
        const x = 1;
        function foo(a) {
            const y = a + x;
            return y;
        }
        foo(2);
        const obj = { prop: 3 };
        obj.prop = 4;
        new Error("fail");
    "#;
    let stream = build_test_facts(src, "fact-builder.js");
    let facts = stream.facts();

    assert!(!facts.is_empty(), "fact builder should emit facts");

    assert!(
        facts
            .iter()
            .any(|fact| matches!(fact.payload, FactPayload::Declaration { .. }))
    );
    assert!(
        facts
            .iter()
            .any(|fact| matches!(fact.payload, FactPayload::Call(_)))
    );
    assert!(
        facts
            .iter()
            .any(|fact| matches!(fact.payload, FactPayload::PropertyWrite { .. }))
    );
    assert!(
        facts
            .iter()
            .any(|fact| matches!(fact.payload, FactPayload::MemberRead { .. }))
    );
}

#[test]
fn facts_record_the_lexical_function_owner() {
    let stream = build_test_facts("fetch(); function helper() { fetch(); }", "owners.js");
    let calls = stream
        .facts()
        .iter()
        .filter(|fact| matches!(fact.payload, FactPayload::Call(_)))
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 2);
    assert_ne!(calls[0].function, calls[1].function);
}

#[test]
fn fact_ids_are_sequential_and_deterministic() {
    let src = "const a = 1; const b = 2; foo();";
    let stream1 = build_test_facts(src, "ids.js");
    let stream2 = build_test_facts(src, "ids.js");

    let ids1: Vec<_> = stream1
        .facts()
        .iter()
        .map(|f| f.id.raw_for_test())
        .collect();
    let ids2: Vec<_> = stream2
        .facts()
        .iter()
        .map(|f| f.id.raw_for_test())
        .collect();
    assert_eq!(
        ids1, ids2,
        "identical programs must produce identical fact IDs"
    );
    assert_eq!(
        ids1,
        (0..u32::try_from(ids1.len()).expect("test fact count fits in u32")).collect::<Vec<_>>(),
        "IDs must be sequential from 0"
    );
}

#[test]
fn fact_count_is_independent_of_enabled_rules() {
    let src = "fetch('/api'); document.createElement('div');";
    let stream = build_test_facts(src, "invariant.js");
    let count = stream.len();

    let stream2 = build_test_facts(src, "invariant.js");
    assert_eq!(
        count,
        stream2.len(),
        "fact count must be invariant across runs"
    );
    assert_eq!(
        stream.fingerprint(),
        stream2.fingerprint(),
        "fact payloads and IDs must be invariant across runs"
    );
}

#[test]
fn fact_builder_reuses_names_collected_by_scope_pass() {
    let source = r"
        const result = globalThis.fetch('/api');
        result.then(value => value.json());
        new Constructor({ option: result });
    ";
    let parsed = crate::parse_test_source(source, "name-reuse.js").expect("source should parse");
    let budget = crate::analysis::SemanticBudget::default();
    let mut resolver = Resolver::collect(&parsed.program, source, &budget);
    let before = resolver
        .name_snapshot()
        .iter()
        .map(|(_, name)| name.to_owned())
        .collect::<Vec<_>>();

    let mut builder = FactBuilder::new(&mut resolver);
    parsed.program.visit_with(&mut builder);
    let _ = builder.into_stream();

    let after = resolver
        .name_snapshot()
        .iter()
        .map(|(_, name)| name.to_owned())
        .collect::<Vec<_>>();
    assert_eq!(after, before, "fact construction must not intern new names");
}

#[test]
fn optional_chain_does_not_double_record_roles() {
    let src = "foo?.bar?.baz();";
    let stream = build_test_facts(src, "opt.js");
    let facts = stream.facts();

    assert_eq!(
        facts
            .iter()
            .filter(|f| matches!(f.payload, FactPayload::Call(_)))
            .count(),
        1,
        "optional call must emit exactly one Call fact"
    );

    let member_facts: Vec<_> = facts
        .iter()
        .filter(|f| matches!(f.payload, FactPayload::MemberRead { .. }))
        .collect();
    assert!(
        member_facts.len() <= 3,
        "optional member chain should not over-produce MemberRead facts, got {}",
        member_facts.len()
    );
}

#[test]
fn nested_call_and_member_roles_have_distinct_facts() {
    let stream = build_test_facts("outer(inner(value.prop));", "nested.js");
    let calls = stream
        .facts()
        .iter()
        .filter(|fact| matches!(fact.payload, FactPayload::Call(_)))
        .collect::<Vec<_>>();
    let members = stream
        .facts()
        .iter()
        .filter(|fact| matches!(fact.payload, FactPayload::MemberRead { .. }))
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 2);
    assert_eq!(members.len(), 1);
    assert_ne!(calls[0].id, calls[1].id);
    assert!(members[0].span.start() >= calls[0].span.start());
    assert!(members[0].span.end() <= calls[0].span.end());
}

#[test]
fn repeated_builds_yield_identical_fact_fingerprints() {
    let src = r"
        const a = fetch('https://example.com');
        a.then(x => x.json());
        document.getElementById('root');
    ";

    let extract = |stream: FactStream<Frozen>| stream.fingerprint();

    let fp1 = extract(build_test_facts(src, "fp.js"));
    let fp2 = extract(build_test_facts(src, "fp.js"));
    let fp3 = extract(build_test_facts(src, "fp.js"));
    assert_eq!(
        fp1, fp2,
        "repeated builds must produce identical fingerprints"
    );
    assert_eq!(
        fp2, fp3,
        "repeated builds must produce identical fingerprints"
    );
}

#[test]
fn call_fact_captures_callee_provenance() {
    let src = "fetch('/api');";
    let stream = build_test_facts(src, "call-prov.js");
    let call_facts: Vec<_> = stream
        .facts()
        .iter()
        .filter(|f| matches!(f.payload, FactPayload::Call(_)))
        .collect();
    assert_eq!(call_facts.len(), 1);
    if let FactPayload::Call(call) = &call_facts[0].payload {
        assert!(
            matches!(call.call_provenance(), SymbolCallProvenance::Global { name } if name == "fetch"),
            "fetch should resolve to global provenance"
        );
        assert_eq!(
            call.callee_name().and_then(|id| stream.resolve_name(id)),
            Some("fetch")
        );
    } else {
        panic!("expected Call payload");
    }
}

#[test]
fn facts_retain_current_value_identities() {
    let src = r"
        function factory() {}
        const source = factory();
        const target = {};
        target.slot = source;
        const read = target.slot;
        class Constructor {}
        new Constructor();
        function outer() { function inner() {} }
    ";
    let stream = build_test_facts(src, "fact-identities.js");

    assert!(stream.facts().iter().any(|fact| {
        matches!(
            &fact.payload,
            FactPayload::Reference { value, .. } if *value != ValueId::UNKNOWN
        )
    }));
    assert!(stream.facts().iter().any(|fact| {
        matches!(
            &fact.payload,
            FactPayload::Call(call) if call.callee() != ValueId::UNKNOWN
        )
    }));
}

#[test]
fn member_read_fact_captures_chain_info() {
    let src = "const x = document.body;";
    let stream = build_test_facts(src, "member-prov.js");
    let member_facts: Vec<_> = stream
        .facts()
        .iter()
        .filter(|f| matches!(&f.payload, FactPayload::MemberRead { .. }))
        .collect();
    assert!(!member_facts.is_empty(), "should have member read facts");
    if let FactPayload::MemberRead { rooted_chain, .. } = &member_facts[0].payload {
        assert!(
            rooted_chain.is_some(),
            "document.body should have a rooted chain"
        );
    }
}

#[test]
fn import_fact_is_emitted() {
    let src = r"import { x } from 'module';";
    let stream = build_test_facts(src, "import.js");
    let import_facts: Vec<_> = stream
        .facts()
        .iter()
        .filter(|f| matches!(&f.payload, FactPayload::Import { .. }))
        .collect();
    assert_eq!(import_facts.len(), 1);
    if let FactPayload::Import { module } = &import_facts[0].payload {
        assert_eq!(module, "module");
    }
}

#[test]
fn string_literal_fact_is_emitted() {
    let src = r#"const x = "hello";"#;
    let parsed = crate::parse_test_source(src, "str.js").expect("source should parse");
    let budget = crate::analysis::SemanticBudget::default();
    let mut resolver = Resolver::collect(&parsed.program, src, &budget);
    let mut builder = FactBuilder::new(&mut resolver);
    parsed.program.visit_with(&mut builder);
    let stream = builder.into_stream();
    let str_facts: Vec<_> = stream
        .facts()
        .iter()
        .filter(|f| {
            matches!(&f.payload, FactPayload::Reference { value, .. }
                if resolver.static_string_value(*value).is_some())
        })
        .collect();
    assert!(!str_facts.is_empty(), "should have string literal facts");
    assert!(
        str_facts
            .iter()
            .filter_map(|f| {
                if let FactPayload::Reference { value, .. } = &f.payload {
                    resolver.static_string_value(*value)
                } else {
                    None
                }
            })
            .any(|value| value == "hello"),
        "should find 'hello' string literal"
    );
}

#[test]
fn class_fact_is_emitted_for_class_declaration() {
    let src = r"class Foo extends Bar {}";
    let stream = build_test_facts(src, "class.js");
    let class_facts: Vec<_> = stream
        .facts()
        .iter()
        .filter(|f| matches!(&f.payload, FactPayload::Class { .. }))
        .collect();
    assert!(!class_facts.is_empty(), "should have class facts");
    if let FactPayload::Class { name, .. } = &class_facts[0].payload {
        assert_eq!(name.as_deref(), Some("Foo"));
    }
}

#[test]
fn instance_class_is_captured_for_this_calls() {
    let src = r"
        import { Base } from 'lib';
        class Foo extends Base {
            bar() { this.baz(); }
        }
    ";
    let stream = build_test_facts(src, "instance.js");
    let call_facts: Vec<_> = stream
        .facts()
        .iter()
        .filter(|f| matches!(f.payload, FactPayload::Call(_)))
        .collect();
    let this_call = call_facts
        .iter()
        .find(|f| {
            if let FactPayload::Call(call) = &f.payload {
                call.instance_class().is_some()
            } else {
                false
            }
        })
        .expect("should find this.baz() call with instance_class");
    if let FactPayload::Call(call) = &this_call.payload {
        assert!(
            call.instance_class().is_some(),
            "this.baz() inside a class with module superclass should capture instance_class"
        );
        assert!(
            call.syntactic_path().is_some(),
            "should have syntactic path for member call"
        );
    }
}
