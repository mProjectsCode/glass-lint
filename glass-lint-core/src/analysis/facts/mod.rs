//! Semantic fact orchestration over one immutable stream.

use self::build::FactBuilder;
use super::flow::projector as object_flow;
use super::matching::MatcherFacts;
use super::resolution::Resolver;
#[cfg(test)]
use super::syntax::SymbolCallProvenance;
#[cfg(test)]
use super::value::{FunctionId, ValueId};
use crate::api::classification::ApiEvidence;
use crate::api::rule::ApiMatcher;
use std::collections::BTreeSet;
#[cfg(test)]
use swc_common::Span;
use swc_ecma_ast::Program;

#[path = "build/mod.rs"]
pub(super) mod build;
mod model;
#[path = "stream.rs"]
mod stream;
pub(in crate::analysis) use model::*;
pub(in crate::analysis) use stream::FactStream;

// ── SemanticFacts ───────────────────────────────────────────────────────

#[derive(Debug)]
pub(in crate::analysis) struct SemanticFacts {
    #[allow(dead_code)]
    pub(in crate::analysis) stream: FactStream,
    pub(in crate::analysis) index: MatcherFacts,
    pub(in crate::analysis) argument_evidence: Vec<Vec<ApiEvidence>>,
    selected: BTreeSet<usize>,
}

impl SemanticFacts {
    pub(in crate::analysis) fn build(
        program: &Program,
        resolver: Resolver,
        matchers: &[&ApiMatcher],
        selected: &[usize],
    ) -> Self {
        let selected = selected.iter().copied().collect::<BTreeSet<_>>();
        let active_matchers = matchers
            .iter()
            .enumerate()
            .filter(|(rule_index, _)| selected.contains(rule_index))
            .collect::<Vec<_>>();
        let member_argument_matchers = active_matchers
            .iter()
            .copied()
            .flat_map(|(rule_index, matcher)| {
                matcher
                    .member_calls
                    .iter()
                    .filter(|matcher| {
                        !matcher.arg_strings.is_empty()
                            || !matcher.arg_object_keys.is_empty()
                            || !matcher.arg_rooted_exprs.is_empty()
                    })
                    .map(move |matcher| (rule_index, matcher))
            })
            .collect::<Vec<_>>();
        let call_argument_matchers = active_matchers
            .iter()
            .copied()
            .flat_map(|(rule_index, matcher)| {
                matcher
                    .calls
                    .iter()
                    .filter(|matcher| !matcher.arg_strings.is_empty())
                    .map(move |matcher| (rule_index, matcher))
            })
            .collect::<Vec<_>>();
        let flow_matchers = active_matchers
            .iter()
            .copied()
            .flat_map(|(rule_index, matcher)| {
                matcher
                    .flows
                    .iter()
                    .enumerate()
                    .map(move |(flow_index, matcher)| (rule_index, flow_index, matcher))
            })
            .collect::<Vec<_>>();

        // Build the canonical fact stream from the authoritative FactBuilder.
        let mut builder = FactBuilder::new(&resolver);
        swc_ecma_visit::VisitWith::visit_with(program, &mut builder);
        let stream = builder.into_stream();

        if !stream.is_valid() {
            return Self {
                stream,
                index: MatcherFacts::default(),
                argument_evidence: vec![Vec::new(); matchers.len()],
                selected,
            };
        }

        // Project the fact stream into rule-independent occurrence indexes.
        let mut index = MatcherFacts::default();
        index.build_from_stream(&stream);

        // Compute argument evidence from pre-computed fact data.
        let mut argument_evidence = vec![Vec::new(); matchers.len()];
        index.compute_argument_evidence_from_stream(
            &stream,
            &member_argument_matchers,
            &call_argument_matchers,
            &mut argument_evidence,
        );

        for (rule_index, evidence) in object_flow::collect(&stream, &flow_matchers, matchers.len())
            .into_iter()
            .enumerate()
        {
            argument_evidence[rule_index].extend(evidence);
        }
        index.normalize_occurrences();
        Self {
            stream,
            index,
            argument_evidence,
            selected,
        }
    }

    pub(in crate::analysis) fn is_selected(&self, rule_index: usize) -> bool {
        self.selected.contains(&rule_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_common::BytePos;

    fn test_fact(id: u32, kind: FactKind, span: Span) -> SemanticFact {
        SemanticFact {
            id: FactId(id),
            span,
            scope: 0,
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
                },
                FactKind::Function => FactPayload::Function {
                    id: FunctionId(0),
                    owner: FunctionId(0),
                    name: None,
                    parameters: Vec::new(),
                    boundary: FunctionBoundary::Enter,
                },
                FactKind::Control => FactPayload::Control {
                    kind: ControlKind::BranchStart,
                    region: 0,
                },
                _ => FactPayload::Declaration {
                    target: ValueId::UNKNOWN,
                    source: ValueId::UNKNOWN,
                },
            },
        }
    }

    #[test]
    fn exact_lookup_distinguishes_equal_span_kinds_and_ordinals() {
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
            stream.exact_lookup(&ExactEventKey {
                lo: span.lo(),
                hi: span.hi(),
                kind: FactKind::Call,
                ordinal: 1,
            }),
            vec![FactId(2)]
        );
    }

    #[test]
    fn dense_exact_lookup_preserves_every_same_span_fact() {
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
            stream.exact_lookup(&ExactEventKey {
                lo: span.lo(),
                hi: span.hi(),
                kind: FactKind::Call,
                ordinal: 10_000,
            }),
            vec![FactId(10_000)]
        );
    }

    #[test]
    fn catalog_selection_and_order_cannot_change_fact_fingerprint() {
        let source = "fetch('/api'); document.createElement('script');";
        let parsed = crate::parse(source, "catalog-fingerprint.js").expect("source should parse");
        let first =
            ApiMatcher::from_matchers(vec![crate::api::rule::Matcher::global_call("fetch")])
                .normalized();
        let second = ApiMatcher::from_matchers(vec![crate::api::rule::Matcher::member_call(
            crate::api::rule::MemberCallMatcher::syntactic_heuristic("document.createElement"),
        )])
        .normalized();
        let build = |matchers: Vec<&ApiMatcher>, selected: &[usize]| {
            let resolver = Resolver::collect(&parsed.program);
            SemanticFacts::build(&parsed.program, resolver, &matchers, selected)
                .stream
                .fingerprint()
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
        let src = r#"
            function fetch(url) { return url; }
            fetch.call(null, '/api');
            fetch.apply(null, ['/api']);
        "#;
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
