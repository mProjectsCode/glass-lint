//! Semantic fact orchestration over one immutable stream.

use self::build::FactBuilder;
use super::flow::projector as object_flow;
use super::matching::MatcherFacts;
use super::module::ModuleInterface;
use super::resolution::Resolver;
#[cfg(test)]
use super::syntax::SymbolCallProvenance;
#[cfg(test)]
use super::value::{FunctionId, ValueId};
use crate::api::compiler::CompiledMatcherCatalog;
#[cfg(test)]
use swc_common::Span;
use swc_ecma_ast::Program;

pub(super) mod build;
mod model;
mod stream;

pub(in crate::analysis) use model::*;
pub(in crate::analysis) use stream::FactStream;

// ── SemanticFacts ───────────────────────────────────────────────────────

#[derive(Debug)]
pub(in crate::analysis) struct SemanticFacts {
    pub(in crate::analysis) stream: FactStream,
    pub(in crate::analysis) index: MatcherFacts,
    pub(in crate::analysis) interface: ModuleInterface,
}

impl SemanticFacts {
    pub(in crate::analysis) fn build(program: &Program, resolver: &Resolver) -> Self {
        // Build the canonical fact stream from the authoritative FactBuilder.
        let mut builder = FactBuilder::new(resolver);
        swc_ecma_visit::VisitWith::visit_with(program, &mut builder);
        let (stream, interface) = builder.into_parts();

        // Project the fact stream into rule-independent occurrence indexes.
        let mut index = MatcherFacts::default();
        if stream.is_valid() {
            index.build_from_stream(&stream);
            index.normalize_occurrences();
        }

        Self {
            stream,
            index,
            interface,
        }
    }

    /// Projects matcher-specific argument and flow evidence after linking.
    pub(in crate::analysis) fn project(
        &self,
        matchers: &CompiledMatcherCatalog<'_>,
        identities: Option<
            &std::collections::BTreeMap<(String, String), super::matching::LinkedModuleIdentity>,
        >,
        result_identities: Option<
            &std::collections::BTreeMap<
                super::value::ValueId,
                super::matching::LinkedModuleIdentity,
            >,
        >,
    ) -> Vec<Vec<crate::api::classification::ApiEvidence>> {
        let member_argument_matchers = matchers
            .selected_matchers()
            .flat_map(|(rule_index, matcher)| {
                matcher
                    .matcher
                    .member_calls
                    .iter()
                    .filter(|matcher| !matcher.arguments.is_empty())
                    .map(move |matcher| (rule_index, matcher))
            })
            .collect::<Vec<_>>();
        let call_argument_matchers = matchers
            .selected_matchers()
            .flat_map(|(rule_index, matcher)| {
                matcher
                    .matcher
                    .calls
                    .iter()
                    .filter(|matcher| !matcher.arguments.is_empty())
                    .map(move |matcher| (rule_index, matcher))
            })
            .collect::<Vec<_>>();
        let flow_matchers = matchers
            .selected_matchers()
            .flat_map(|(rule_index, matcher)| {
                matcher
                    .flows
                    .iter()
                    .enumerate()
                    .map(move |(flow_index, matcher)| (rule_index, flow_index, matcher))
            })
            .collect::<Vec<_>>();

        let mut argument_evidence = vec![Vec::new(); matchers.len()];
        if !self.stream.is_valid() {
            return argument_evidence;
        }
        MatcherFacts::compute_argument_evidence_from_stream_with_overlay(
            &self.stream,
            &member_argument_matchers,
            &call_argument_matchers,
            &mut argument_evidence,
            identities,
            result_identities,
        );
        for (rule_index, evidence) in
            object_flow::collect(&self.stream, &flow_matchers, matchers.len())
                .into_iter()
                .enumerate()
        {
            argument_evidence[rule_index].extend(evidence);
        }
        argument_evidence
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    use crate::api::compiler::{CompiledMatcherCatalog, CompiledMatcherPlan};
    use crate::api::rule::ApiMatcher;
    use swc_common::BytePos;

    fn test_fact(id: u32, kind: FactKind, span: Span) -> SemanticFact {
        SemanticFact {
            id: FactId(id),
            span,
            function: FunctionId(0),
            kind,
            payload: match kind {
                FactKind::Call => FactPayload::Call {
                    callee: ValueId::UNKNOWN,
                    receiver: None,
                    result: ValueId::UNKNOWN,
                    callee_span: span,
                    callee_name: None,
                    call_provenance: SymbolCallProvenance::Local,
                    syntactic_chain: None,
                    rooted_chain: None,
                    module_member: None,
                    returned_member: None,
                    instance_class: None,
                    target_function: None,
                    args: Vec::new(),
                    unwrap: None,
                },
                FactKind::MemberRead => FactPayload::MemberRead {
                    value: ValueId::UNKNOWN,
                    syntactic_chain: None,
                    rooted_chain: None,
                    module_member: None,
                    returned_member: None,
                },
                FactKind::Reference => FactPayload::Reference {
                    value: ValueId::UNKNOWN,
                    static_string: None,
                    provenance: SymbolCallProvenance::Local,
                },
                FactKind::Function => FactPayload::Function {
                    id: FunctionId(0),
                    owner: FunctionId(0),
                    parameters: Vec::new(),
                    boundary: FunctionBoundary::Enter,
                },
                FactKind::Control => FactPayload::Control {
                    kind: ControlKind::BranchStart,
                    region: 0,
                    value: ValueId::UNKNOWN,
                },
                _ => FactPayload::Declaration {
                    target: ValueId::UNKNOWN,
                    source: ValueId::UNKNOWN,
                },
            },
        }
    }

    #[test]
    fn direct_lookup_and_linear_test_helper_preserve_fact_order() {
        let span = Span::new(BytePos(10), BytePos(20));
        let mut stream = FactStream::new();
        stream.push(test_fact(0, FactKind::Call, span));
        stream.push(test_fact(1, FactKind::MemberRead, span));
        stream.push(test_fact(2, FactKind::Call, span));

        assert_eq!(
            stream
                .facts_at(span.lo(), span.hi(), FactKind::Call)
                .iter()
                .map(|fact| fact.id)
                .collect::<Vec<_>>(),
            vec![FactId(0), FactId(2)]
        );
        assert_eq!(
            stream.fact(FactId(0)).map(|fact| fact.kind),
            Some(FactKind::Call)
        );
        assert_eq!(
            stream.fact(FactId(2)).map(|fact| fact.kind),
            Some(FactKind::Call)
        );
        assert!(stream.fact(FactId(3)).is_none());
    }

    #[test]
    fn dense_fact_stream_preserves_every_same_span_fact() {
        let span = Span::new(BytePos(100), BytePos(120));
        let mut stream = FactStream::new();
        for id in 0..10_001 {
            stream.push(test_fact(id, FactKind::Call, span));
        }
        let calls = stream.facts_at(span.lo(), span.hi(), FactKind::Call);
        assert_eq!(calls.len(), 10_001);
        assert_eq!(calls.first().map(|fact| fact.id), Some(FactId(0)));
        assert_eq!(calls.last().map(|fact| fact.id), Some(FactId(10_000)));
        assert_eq!(
            stream.fact(FactId(10_000)).map(|fact| fact.id),
            Some(FactId(10_000))
        );
    }

    #[test]
    fn fact_ids_have_checked_collection_boundaries() {
        assert_eq!(FactId::from_index(0), Some(FactId(0)));
        assert_eq!(
            FactId::from_index(MAX_FACTS - 1),
            Some(FactId(
                u32::try_from(MAX_FACTS - 1).expect("fact limit fits in FactId")
            ))
        );
        assert_eq!(FactId::from_index(MAX_FACTS), None);
        assert_eq!(FactId(u32::MAX).index(), None);
    }

    #[test]
    fn catalog_selection_and_order_cannot_change_fact_index() {
        let source = "fetch('/api'); document.createElement('script');";
        let parsed = crate::parse(source, "catalog-fingerprint.js").expect("source should parse");
        let first =
            ApiMatcher::from_matchers(vec![crate::api::rule::Matcher::global_call("fetch")])
                .normalized();
        let second = ApiMatcher::from_matchers(vec![crate::api::rule::Matcher::member_call(
            crate::api::rule::MemberCallMatcher::syntactic_heuristic("document.createElement"),
        )])
        .normalized();
        let first = CompiledMatcherPlan::compile(&first);
        let second = CompiledMatcherPlan::compile(&second);
        let build = |matchers: Vec<&crate::api::compiler::CompiledMatcherPlan>,
                     selected: &[usize]| {
            let resolver = Resolver::collect(&parsed.program);
            let selected = selected.iter().copied().collect::<BTreeSet<_>>();
            let _catalog = CompiledMatcherCatalog::new(matchers, &selected);
            format!(
                "{:?}",
                SemanticFacts::build(&parsed.program, &resolver).index
            )
        };

        let forward = build(vec![&first, &second], &[0, 1]);
        assert_eq!(forward, build(vec![&first, &second], &[0]));
        assert_eq!(forward, build(vec![&first, &second], &[1, 0]));
        assert_eq!(forward, build(vec![&first, &second], &[]));
        assert_eq!(forward, build(vec![&second, &first], &[0, 1]));
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
        let parsed = crate::parse(src, "char-index.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);

        let mut builder = FactBuilder::new(&resolver);
        swc_ecma_visit::VisitWith::visit_with(&parsed.program, &mut builder);
        let stream = builder.into_stream();
        let mut index = MatcherFacts::default();
        index.build_from_stream(&stream);
        index.normalize_occurrences();

        assert!(
            index.imports.get("mod").is_some(),
            "should have 'mod' import"
        );
        assert!(
            index.imports.get("other-mod").is_some(),
            "should have 'other-mod' import"
        );
        assert!(
            index.imports.get("path").is_some(),
            "should have 'path' require import"
        );
        assert!(index.calls.get("greet").is_some(), "should have greet call");
        assert!(
            index.string_literals.get("world").is_some(),
            "should have 'world' string literal"
        );
        assert!(!index.classes.is_empty(), "should have class entries");
        assert!(
            index
                .module_classes
                .get(&("other-mod".to_string(), "Bar".to_string()))
                .is_some(),
            "should have module class for Bar from other-mod"
        );
        assert!(
            !index.constructors.is_empty(),
            "should have constructor entries"
        );
        assert!(
            index.member_calls.get("x.hello").is_some()
                || index.rooted_member_calls.iter().next().is_some()
                || index.member_calls.iter().next().is_some(),
            "should have member calls"
        );
    }

    /// Verify that .call()/.apply() unwrapping produces the expected
    /// member call entries for the target.
    #[test]
    fn call_apply_unwrapping_populates_indexes() {
        let src = r"
            function fetch(url) { return url; }
            fetch.call(null, '/api');
            fetch.apply(null, ['/api']);
        ";
        let parsed = crate::parse(src, "unwrap.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);

        let mut builder = FactBuilder::new(&resolver);
        swc_ecma_visit::VisitWith::visit_with(&parsed.program, &mut builder);
        let stream = builder.into_stream();
        let mut index = MatcherFacts::default();
        index.build_from_stream(&stream);
        index.normalize_occurrences();

        // The unwrap should record 'fetch' as a member call.
        assert!(
            index.member_calls.get("fetch").is_some(),
            "should have 'fetch' as member call from unwrapping"
        );
    }
}
