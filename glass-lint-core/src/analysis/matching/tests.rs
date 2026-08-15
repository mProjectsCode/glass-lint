use glass_lint_datastructures::{ByteRange, SymbolPath};

use super::*;
use crate::{
    Environment,
    analysis::{
        facts::{FactId, build_test_stream},
        matching::occurrence::OccurrenceIndex,
        resolution::Resolver,
    },
    api::{compiler::rule::CompiledMatcherPlan, rule::EventQuery},
};

fn span(start: u32, end: u32) -> ByteRange {
    ByteRange::new(start, end).unwrap()
}

#[test]
fn typed_occurrence_index_is_deduplicated() {
    let mut index = OccurrenceIndex::<SmolStr>::default();
    index.push("fetch".into(), FactId::from_test(2), span(20, 26));
    index.push("fetch".into(), FactId::from_test(1), span(5, 11));
    index.push("fetch".into(), FactId::from_test(1), span(5, 11));
    index.normalize();
    assert_eq!(
        index
            .get("fetch")
            .unwrap()
            .iter()
            .map(Occurrence::span)
            .collect::<Vec<_>>(),
        vec![span(5, 11), span(20, 26)]
    );
}

#[test]
fn optimized_member_query_matches_reference_occurrences() {
    let mut facts = OccurrenceIndexes::default();
    facts.record(MatchKind::MemberCall, "client.request", span(30, 44));
    facts.record(MatchKind::MemberCall, "other.request", span(5, 18));
    facts.record(MatchKind::MemberCall, "client.request", span(10, 24));
    facts.normalize_occurrences();

    let compiled =
        CompiledMatcherPlan::compile(&[EventQuery::member_call_heuristic("client.request")
            .unwrap()
            .into_query()])
        .unwrap();
    let evidence = facts.evidence_for(&compiled);
    let reference = facts
        .members
        .calls()
        .iter()
        .filter(|(symbol, _)| {
            facts
                .test_names
                .resolve_path(symbol)
                .is_some_and(|symbol| symbol == SymbolPath::from_chain("client.request"))
        })
        .flat_map(|(_, occurrences)| occurrences.iter().map(Occurrence::span))
        .collect::<Vec<_>>();
    assert_eq!(evidence.len(), 1);
    assert_eq!(
        evidence[0]
            .occurrences()
            .iter()
            .map(crate::api::classification::ClassificationEvidenceOccurrence::span)
            .collect::<Vec<_>>(),
        reference
    );
}

#[test]
fn unknown_namespace_wildcard_masks_base_module_occurrences() {
    let key = ModuleExportKey::new("namespace", "request");
    let mut indexes = OccurrenceIndexes::default();
    indexes.call_indexes.record_module_call(
        key.clone(),
        Occurrence::new(FactId::from_test(1), span(5, 12)),
    );
    indexes.normalize_occurrences();

    let mut identities = ModuleIdentityMap::new();
    identities.insert(
        ModuleExportKey::wildcard("namespace"),
        ExportResolution::Unknown,
    );
    let (view, _) = LinkedOccurrenceView::build(&indexes, &identities);

    assert!(
        view.resolve_module(
            ModuleOverlayKind::Call,
            indexes.call_indexes.module_calls(),
            &key,
        )
        .is_none()
    );
}

#[test]
fn build_from_stream_populates_all_occurrence_indexes() {
    let src = r#"
            import { foo } from 'mod';
            import { Bar } from 'other-mod';
            class MyClass extends Bar {}
            const x = foo;
            foo();
            x.hello();
            new MyClass();
            new URL("https://example.com");
            const s = "hello world";
            require('fs');
        "#;
    let parsed = crate::parse_test_source(src, "stream-index.js").expect("source should parse");
    let mut environment = Environment::default();
    environment
        .add_globals(["URL", "require"])
        .expect("test globals");
    let budget = crate::analysis::SemanticBudget::default();
    let mut resolver = Resolver::collect_with_environment(
        &parsed.program,
        &environment,
        crate::analysis::semantic::SpanNormalizer::for_program(&parsed.program, src),
        &budget,
    );
    let stream = build_test_stream(&parsed.program, &mut resolver);

    let index =
        OccurrenceIndexes::from_stream(&stream, &environment, DerivedPhaseAvailability::Enabled);

    assert!(
        index.literals.imports().get("mod").is_some(),
        "should have 'mod' import"
    );
    assert!(
        index.literals.imports().get("other-mod").is_some(),
        "should have 'other-mod' import"
    );
    assert!(
        index.literals.imports().get("fs").is_some(),
        "should have 'fs' require import"
    );

    assert!(
        index.literals.strings().get("hello world").is_some(),
        "should have 'hello world' string literal"
    );

    assert!(
        index.constructions.classes().get("MyClass").is_some(),
        "should have MyClass class"
    );

    assert!(index.has_constructor("URL"), "should have URL constructor");

    assert!(index.has_call("foo"), "should have foo call");
    assert!(
        index
            .call_indexes
            .module_calls()
            .get(&ModuleExportKey::new("mod", "foo"))
            .is_some(),
        "should have foo as module call from 'mod'"
    );
    assert!(
        index
            .members
            .module_calls()
            .get(&ModuleExportKey::new("mod", "foo"))
            .is_some(),
        "should have foo as member module call from 'mod'"
    );
}
