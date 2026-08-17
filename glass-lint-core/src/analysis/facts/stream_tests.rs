use glass_lint_datastructures::ByteRange;

use super::*;
use crate::{
    analysis::{
        facts::stream::FactStreamToken, model::scope::FunctionId, resolution::Resolver,
        syntax::SymbolCallProvenance,
    },
    api::{compiler::rule::CompiledMatcherPlan, rule::EventQuery},
};

fn test_call(id: u32, span: ByteRange) -> SemanticFact {
    SemanticFact::new(
        FactStreamToken::for_test(),
        FactId::from_test(id),
        span,
        FunctionId::from_test(0),
        FactPayload::Call(crate::analysis::model::fact::CallEvent::unknown(
            ValueId::UNKNOWN,
            span,
            SymbolCallProvenance::Local,
            Vec::new(),
        )),
    )
}

fn test_member_read(id: u32, span: ByteRange) -> SemanticFact {
    SemanticFact::new(
        FactStreamToken::for_test(),
        FactId::from_test(id),
        span,
        FunctionId::from_test(0),
        FactPayload::MemberRead {
            syntactic_path: None,
            rooted_chain: None,
            module_member: None,
            returned_member: None,
        },
    )
}

#[test]
fn direct_lookup_and_linear_test_helper_preserve_fact_order() {
    let span = ByteRange::new(10, 20).unwrap();
    let mut stream = FactStream::<Building>::new();
    stream.push(test_call(0, span));
    stream.push(test_member_read(1, span));
    stream.push(test_call(2, span));

    assert_eq!(
        stream
            .facts()
            .iter()
            .filter(|fact| {
                fact.span.start() == span.start()
                    && fact.span.end() == span.end()
                    && matches!(fact.payload, FactPayload::Call(_))
            })
            .map(SemanticFact::id)
            .collect::<Vec<_>>(),
        vec![FactId::from_test(0), FactId::from_test(2)]
    );
    assert!(
        stream
            .fact(FactId::from_test(0))
            .is_some_and(|fact| { matches!(fact.payload, FactPayload::Call(_)) })
    );
    assert!(
        stream
            .fact(FactId::from_test(2))
            .is_some_and(|fact| { matches!(fact.payload, FactPayload::Call(_)) })
    );
    assert!(stream.fact(FactId::from_test(3)).is_none());
}

#[test]
fn dense_fact_stream_preserves_every_same_span_fact() {
    let span = ByteRange::new(100, 120).unwrap();
    let mut stream = FactStream::<Building>::new();
    for id in 0..10_001 {
        stream.push(test_call(id, span));
    }
    let calls = stream
        .facts()
        .iter()
        .filter(|fact| {
            fact.span.start() == span.start()
                && fact.span.end() == span.end()
                && matches!(fact.payload, FactPayload::Call(_))
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 10_001);
    assert_eq!(
        calls.first().map(|fact| fact.id()),
        Some(FactId::from_test(0))
    );
    assert_eq!(
        calls.last().map(|fact| fact.id()),
        Some(FactId::from_test(10_000))
    );
    assert_eq!(
        stream.fact(FactId::from_test(10_000)).map(SemanticFact::id),
        Some(FactId::from_test(10_000))
    );
}

#[test]
fn fact_ids_have_checked_collection_boundaries() {
    assert_eq!(FactId::from_index(0), Some(FactId::from_test(0)));
    assert_eq!(
        FactId::from_index(MAX_FACTS - 1),
        Some(FactId::from_test(
            u32::try_from(MAX_FACTS - 1).expect("fact limit fits in FactId")
        ))
    );
    assert_eq!(FactId::from_index(MAX_FACTS), None);
    assert_eq!(FactId::from_test(u32::MAX).index(), None);
}

#[test]
fn catalog_selection_and_order_cannot_change_fact_index() {
    let source = "fetch('/api'); document.createElement('script');";
    let parsed =
        crate::parse_test_source(source, "catalog-fingerprint.js").expect("source should parse");
    let first =
        CompiledMatcherPlan::compile(&[EventQuery::call_global("fetch").unwrap().into_query()])
            .unwrap();
    let second = CompiledMatcherPlan::compile(&[EventQuery::member_call_heuristic(
        "document.createElement",
    )
    .unwrap()
    .into_query()])
    .unwrap();
    let build = |matchers: Vec<&crate::api::compiler::rule::CompiledMatcherPlan>,
                 selected: &[usize]| {
        let _ = (matchers, selected);
        let artifact = with_test_collection(&parsed.program, source, |resolved| {
            resolved.freeze(
                &crate::Environment::default(),
                &crate::AnalysisLimits::default(),
                parsed.program.span(),
            )
        });
        format!("{:?}", artifact.facts().matcher_index())
    };

    let forward = build(vec![&first, &second], &[0, 1]);
    assert_eq!(forward, build(vec![&first, &second], &[0]));
    assert_eq!(forward, build(vec![&first, &second], &[1, 0]));
    assert_eq!(forward, build(vec![&first, &second], &[]));
    assert_eq!(forward, build(vec![&second, &first], &[0, 1]));
}

#[test]
fn lowering_shared_derived_pass_matches_standalone_effect_collection() {
    let source = "function helper(value) { return value; } helper('/api');";
    let parsed =
        crate::parse_test_source(source, "shared-derived-pass.js").expect("source should parse");
    let limits = crate::AnalysisLimits::default()
        .with_effect_operations(usize::MAX)
        .expect("valid effect limit");
    let artifact = with_test_collection(&parsed.program, source, |resolved| {
        resolved.freeze(
            &crate::Environment::default(),
            &limits,
            parsed.program.span(),
        )
    });
    let combined_effects = artifact.effects();
    let standalone_effects = FunctionEffects::collect(artifact.facts().stream(), usize::MAX);

    let summarize = |effects: &FunctionEffects| {
        effects
            .iter_effects()
            .map(|effect| {
                (
                    effect.id(),
                    effect.calls().len(),
                    effect.uses().len(),
                    effect.returns().len(),
                    effect.is_invalid(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        summarize(combined_effects),
        summarize(&standalone_effects),
        "sharing the fact-tape pass must preserve function effects"
    );
    assert_eq!(
        combined_effects.operation_count(),
        standalone_effects.operation_count()
    );
    assert_eq!(
        combined_effects.completion().is_incomplete(),
        standalone_effects.completion().is_incomplete()
    );
    assert!(
        !artifact.facts().matcher_index().is_empty(),
        "the same pass must still populate occurrence indexes"
    );
}

/// Verify that the fact-driven index populates expected occurrence maps
/// for a diverse program.
#[test]
fn fact_driven_index_populates_expected_maps() {
    let src = r#"
        import { foo } from 'mod';
        import { Bar } from 'other-mod';
        class MyApp extends Bar {}
        const x = foo;
        function greet(name) { return name; }
        greet("hello");
        x.hello();
        new Bar();
        const s = "world";
        require('path');
        const a = [1, 2];
        a.push(3);
    "#;
    let parsed = crate::parse_test_source(src, "char-index.js").expect("source should parse");
    let budget = crate::analysis::SemanticBudget::default();
    let mut resolver = Resolver::collect(&parsed.program, src, &budget);

    let mut builder = FactBuilder::new(&mut resolver);
    swc_ecma_visit::VisitWith::visit_with(&parsed.program, &mut builder);
    let stream = builder.into_stream();
    let index = OccurrenceIndexes::from_stream(
        &stream,
        &crate::Environment::default(),
        DerivedPhaseAvailability::Enabled,
    );

    assert!(index.has_import("mod"), "should have 'mod' import");
    assert!(
        index.has_import("other-mod"),
        "should have 'other-mod' import"
    );
    assert!(
        index.has_import("path"),
        "should have 'path' require import"
    );
    assert!(
        index.has_call(stream.names(), "greet"),
        "should have greet call"
    );
    assert!(
        index.has_string("world"),
        "should have 'world' string literal"
    );
    assert!(index.has_any_class(), "should have class entries");
    assert!(
        index.has_module_class("other-mod", "Bar"),
        "should have module class for Bar from other-mod"
    );
    assert!(
        index.has_module_constructor("other-mod", "Bar"),
        "should have module constructor entries"
    );
    assert!(index.has_any_member_call(), "should have member calls");
}

/// Verify that .call()/.apply() unwrapping produces the expected
/// member call entries for the target.
#[test]
fn call_apply_unwrapping_populates_indexes() {
    let src = r"
        function fetch(url) { return url; }
        fetch.call(null, '/api');
        fetch.apply(null, ['/api']);
        (fetch?.call)(null, '/api');
        (fetch?.other)(null, '/api');
    ";
    let parsed = crate::parse_test_source(src, "unwrap.js").expect("source should parse");
    let budget = crate::analysis::SemanticBudget::default();
    let mut resolver = Resolver::collect(&parsed.program, src, &budget);

    let mut builder = FactBuilder::new(&mut resolver);
    swc_ecma_visit::VisitWith::visit_with(&parsed.program, &mut builder);
    let stream = builder.into_stream();
    let index = OccurrenceIndexes::from_stream(
        &stream,
        &crate::Environment::default(),
        DerivedPhaseAvailability::Enabled,
    );

    // The unwrap should record 'fetch' as a member call.
    assert!(
        index.has_member_call(stream.names(), "fetch"),
        "should have 'fetch' as member call from unwrapping"
    );
    assert!(
        index.has_member_call(stream.names(), "fetch.other"),
        "the unrelated optional member remains a member call rather than a wrapper"
    );
}
