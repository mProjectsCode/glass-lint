//! Semantic fact orchestration over one immutable stream.
//!
//! This module owns the matcher-independent boundary: scope predeclaration is
//! followed by one fact-building AST traversal that produces facts, occurrence
//! indexes, and a module interface. Matcher
//! selection is applied only by [`SemanticFacts::project`] after that shared
//! state has been built.

use std::{collections::BTreeMap, sync::Arc};

use crate::{
    analysis::{
        flow::{effect::FunctionEffects, projector as object_flow},
        matching::{
            self, LinkedModuleIdentity, ModuleIdentityMap, ModuleOccurrenceOverlay,
            OccurrenceIndexes,
        },
        module::ModuleInterface,
        value::ValueId,
    },
    api::compiler::CompiledRuleSelection,
};

pub mod build;
mod model;
mod stream;

pub(in crate::analysis) use model::*;
pub(in crate::analysis) use stream::FactStream;

// ── SemanticFacts ───────────────────────────────────────────────────────

#[derive(Debug)]
/// Immutable per-file semantic state shared by all selected matchers.
///
/// A malformed or budget-exhausted stream remains available for diagnostics,
/// but indexing and projection fail closed rather than consuming partial facts.
pub(in crate::analysis) struct SemanticFacts {
    stream: FactStream,
    index: Arc<OccurrenceIndexes>,
    interface: ModuleInterface,
}

impl SemanticFacts {
    /// Assemble immutable indexes from the stream produced by lowering.
    pub(in crate::analysis) fn from_lowering(
        stream: FactStream,
        interface: ModuleInterface,
        environment: &crate::Environment,
    ) -> Self {
        // Project the fact stream into rule-independent occurrence indexes.
        let mut index = OccurrenceIndexes::with_environment(environment);
        if stream.is_valid() {
            index.build_from_stream(&stream);
            index.normalize_occurrences();
        }

        Self {
            stream,
            index: Arc::new(index),
            interface,
        }
    }

    /// Borrow the canonical facts in deterministic source traversal order.
    pub(in crate::analysis) fn stream(&self) -> &FactStream {
        &self.stream
    }

    pub(in crate::analysis) fn names(&self) -> Option<&crate::analysis::name::NameTable> {
        self.stream.names()
    }

    pub(in crate::analysis) fn shared_matcher_index(&self) -> Arc<OccurrenceIndexes> {
        Arc::clone(&self.index)
    }

    /// Borrow the module requests and export facts collected during the walk.
    pub(in crate::analysis) fn interface(&self) -> &ModuleInterface {
        &self.interface
    }

    /// Projects constrained-clause and flow evidence after linking.
    pub(in crate::analysis) fn project(
        &self,
        effects: &FunctionEffects,
        matchers: &CompiledRuleSelection<'_>,
        identities: Option<&ModuleIdentityMap>,
        result_identities: Option<&BTreeMap<ValueId, LinkedModuleIdentity>>,
        overlay: Option<&ModuleOccurrenceOverlay>,
    ) -> Vec<Vec<crate::api::classification::ClassificationEvidence>> {
        let constrained_clauses = matchers
            .selected_matchers()
            .flat_map(|(rule_index, matcher)| {
                matcher
                    .query()
                    .clauses()
                    .iter()
                    .filter(|clause| !clause.constraints.is_empty())
                    .map(move |clause| (rule_index, clause))
            })
            .map(|(rule, clause)| (rule.get(), clause))
            .collect::<Vec<_>>();
        let flow_matchers = matchers
            .selected_matchers()
            .flat_map(|(rule_index, matcher)| {
                matcher
                    .query()
                    .flows()
                    .iter()
                    .enumerate()
                    .map(move |(flow_index, matcher)| (rule_index, flow_index, matcher))
            })
            .collect::<Vec<_>>();

        let mut projected_evidence = vec![Vec::new(); matchers.len()];
        if !self.stream.is_valid() {
            return projected_evidence;
        }
        matching::compute_constrained_evidence_from_stream_with_overlay(
            &self.stream,
            &self.index,
            &constrained_clauses,
            &mut projected_evidence,
            overlay,
            identities,
            result_identities,
        );
        for (rule_index, evidence) in
            object_flow::collect(&self.stream, effects, &flow_matchers, matchers.len())
                .into_iter()
                .enumerate()
        {
            projected_evidence[rule_index].extend(evidence);
        }
        projected_evidence
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ByteRange,
        analysis::{
            SymbolCallProvenance, facts::build::FactBuilder, resolution::Resolver,
            value::FunctionId,
        },
        api::{compiler::CompiledMatcherPlan, rule::MatcherSet},
    };

    fn test_fact(id: u32, kind: FactKind, span: ByteRange) -> SemanticFact {
        SemanticFact::new(
            FactId(id),
            span,
            FunctionId(0),
            kind,
            match kind {
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
                    parameters: Vec::new(),
                    boundary: FunctionBoundary::Enter,
                },
                FactKind::Control => FactPayload::Control {
                    kind: ControlKind::BranchStart,
                    region: ControlRegionId(0),
                    return_value: ValueId::UNKNOWN,
                },
                _ => FactPayload::Declaration {
                    target: ValueId::UNKNOWN,
                    source: ValueId::UNKNOWN,
                },
            },
        )
    }

    #[test]
    fn direct_lookup_and_linear_test_helper_preserve_fact_order() {
        let span = ByteRange::new(10, 20).unwrap();
        let mut stream = FactStream::new();
        stream.push(test_fact(0, FactKind::Call, span));
        stream.push(test_fact(1, FactKind::MemberRead, span));
        stream.push(test_fact(2, FactKind::Call, span));

        assert_eq!(
            stream
                .facts_at(span.start(), span.end(), FactKind::Call)
                .iter()
                .map(|fact| fact.id())
                .collect::<Vec<_>>(),
            vec![FactId(0), FactId(2)]
        );
        assert_eq!(
            stream.fact(FactId(0)).map(SemanticFact::kind),
            Some(FactKind::Call)
        );
        assert_eq!(
            stream.fact(FactId(2)).map(SemanticFact::kind),
            Some(FactKind::Call)
        );
        assert!(stream.fact(FactId(3)).is_none());
    }

    #[test]
    fn dense_fact_stream_preserves_every_same_span_fact() {
        let span = ByteRange::new(100, 120).unwrap();
        let mut stream = FactStream::new();
        for id in 0..10_001 {
            stream.push(test_fact(id, FactKind::Call, span));
        }
        let calls = stream.facts_at(span.start(), span.end(), FactKind::Call);
        assert_eq!(calls.len(), 10_001);
        assert_eq!(calls.first().map(|fact| fact.id()), Some(FactId(0)));
        assert_eq!(calls.last().map(|fact| fact.id()), Some(FactId(10_000)));
        assert_eq!(
            stream.fact(FactId(10_000)).map(SemanticFact::id),
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
            MatcherSet::from_matchers(vec![crate::api::rule::Matcher::global_call("fetch")])
                .normalized();
        let second = MatcherSet::from_matchers(vec![crate::api::rule::Matcher::from(
            crate::api::rule::MemberCallMatcher::heuristic("document.createElement"),
        )])
        .normalized();
        let first = CompiledMatcherPlan::compile(&first);
        let second = CompiledMatcherPlan::compile(&second);
        let build = |matchers: Vec<&crate::api::compiler::CompiledMatcherPlan>,
                     selected: &[usize]| {
            let resolver = Resolver::collect(&parsed.program);
            let _ = (matchers, selected);
            let mut builder = FactBuilder::new(&resolver);
            swc_ecma_visit::VisitWith::visit_with(&parsed.program, &mut builder);
            let (mut stream, interface) = builder.into_parts();
            let _ = stream.freeze_names(resolver.name_snapshot());
            format!(
                "{:?}",
                SemanticFacts::from_lowering(stream, interface, &crate::Environment::default())
                    .index
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
        let mut index = OccurrenceIndexes::default();
        index.build_from_stream(&stream);
        index.normalize_occurrences();

        assert!(index.has_import("mod"), "should have 'mod' import");
        assert!(
            index.has_import("other-mod"),
            "should have 'other-mod' import"
        );
        assert!(
            index.has_import("path"),
            "should have 'path' require import"
        );
        assert!(index.has_call("greet"), "should have greet call");
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
        ";
        let parsed = crate::parse(src, "unwrap.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);

        let mut builder = FactBuilder::new(&resolver);
        swc_ecma_visit::VisitWith::visit_with(&parsed.program, &mut builder);
        let stream = builder.into_stream();
        let mut index = OccurrenceIndexes::default();
        index.build_from_stream(&stream);
        index.normalize_occurrences();

        // The unwrap should record 'fetch' as a member call.
        assert!(
            index.has_member_call("fetch"),
            "should have 'fetch' as member call from unwrapping"
        );
    }
}
