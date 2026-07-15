//! Rule-independent indexes built from one semantic model.
//!
//! Collection is intentionally separated from `evidence_for`: the AST is
//! walked once, then each rule selects from deterministic occurrence maps.
//! Argument and flow evidence remain per-rule because their matchers carry
//! rule-specific predicates that cannot be represented as a shared key.

use swc_common::Span;

use super::facts::{CallArgInfo, FactId, FactPayload, FactStream};
use super::syntax::{SymbolCallProvenance, SymbolMemberProvenance};
use crate::api::classification::{ApiEvidence, ApiMatchKind};
use crate::api::rule::{
    ApiMatcher, CallMatcher, CallProvenance, ClassMatcher, ConstructorMatcher, MemberCallMatcher,
    MemberCallProvenance, MemberReadMatcher, MemberReadProvenance, canonical_rooted_chain,
};

mod occurrence;
use occurrence::{ModuleOccurrences, Occurrence, OccurrenceIndex, Occurrences};
mod arguments;
mod build;
mod query;

#[derive(Clone, Debug, Default)]
pub struct MatcherFacts {
    // Each map represents a different confidence/provenance level. Do not
    // collapse these into one index: a global spelling, rooted alias, and
    // imported member have intentionally different matching semantics.
    //
    // The fields are deliberately grouped by semantic family rather than by
    // the order in which facts are emitted. That makes it easier to audit a
    // matcher query against the indexes it is allowed to consume.
    pub(super) calls: Occurrences,
    pub(super) global_calls: Occurrences,
    pub(super) module_calls: ModuleOccurrences,
    pub(super) member_calls: Occurrences,
    pub(super) rooted_member_calls: Occurrences,
    pub(super) module_member_calls: ModuleOccurrences,
    pub(super) member_reads: Occurrences,
    pub(super) rooted_member_reads: Occurrences,
    pub(super) module_member_reads: ModuleOccurrences,
    pub(super) returned_member_calls: OccurrenceIndex<(String, String)>,
    pub(super) returned_member_reads: OccurrenceIndex<(String, String)>,
    pub(super) instance_member_calls: OccurrenceIndex<(String, String, String)>,
    pub(super) imports: Occurrences,
    pub(super) string_literals: Occurrences,
    pub(super) classes: Occurrences,
    pub(super) module_classes: ModuleOccurrences,
    pub(super) constructors: Occurrences,
    pub(super) global_constructors: Occurrences,
    pub(super) module_constructors: ModuleOccurrences,
}

/// The only identities a linked module overlay exposes to matcher indexes.
/// Qualified local values and unknown values are intentionally not queryable
/// by the external-module matcher vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::analysis) enum LinkedModuleIdentity {
    External { module: String, export: String },
    Global { name: String },
    Qualified,
    Unknown,
}

impl MatcherFacts {
    pub(in crate::analysis) fn apply_module_overlay(
        &mut self,
        identities: &std::collections::BTreeMap<(String, String), LinkedModuleIdentity>,
    ) {
        let remap = |key: &(String, String)| {
            let identity = identities.get(key).cloned().or_else(|| {
                identities
                    .get(&(key.0.clone(), "*".into()))
                    .map(|identity| match identity {
                        LinkedModuleIdentity::External { module, .. } => {
                            LinkedModuleIdentity::External {
                                module: module.clone(),
                                export: key.1.clone(),
                            }
                        }
                        other => other.clone(),
                    })
            });
            match identity {
                Some(LinkedModuleIdentity::External { module, export }) => Some((module, export)),
                Some(
                    LinkedModuleIdentity::Global { .. }
                    | LinkedModuleIdentity::Qualified
                    | LinkedModuleIdentity::Unknown,
                ) => None,
                None => Some(key.clone()),
            }
        };

        let global_occurrences = self
            .module_calls
            .iter()
            .filter_map(|(key, occurrences)| {
                identities.get(key).and_then(|identity| {
                    if let LinkedModuleIdentity::Global { name } = identity {
                        Some((name.clone(), occurrences.clone()))
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>();

        self.module_calls.remap_keys(remap);
        self.module_member_calls.remap_keys(remap);
        self.module_member_reads.remap_keys(remap);
        self.module_classes.remap_keys(remap);
        self.module_constructors.remap_keys(remap);

        // A callable imported through an internal module can resolve to a
        // global identity (for example `export const f = fetch`). It is safe
        // to add that occurrence to the global index, but never to infer one
        // from a qualified local or unknown export.
        for (name, occurrences) in global_occurrences {
            for occurrence in occurrences {
                self.global_calls
                    .push(name.clone(), occurrence.event, occurrence.span);
            }
        }
        self.global_calls.normalize();
    }
}

fn push_evidence(
    evidence: &mut Vec<ApiEvidence>,
    kind: ApiMatchKind,
    symbol: String,
    occurrences: Option<&Vec<Occurrence>>,
) {
    push_owned_evidence(evidence, kind, symbol, occurrences.cloned());
}

fn push_owned_evidence(
    evidence: &mut Vec<ApiEvidence>,
    kind: ApiMatchKind,
    symbol: String,
    occurrences: Option<Vec<Occurrence>>,
) {
    let Some(occurrences) = occurrences else {
        return;
    };
    if occurrences.is_empty() {
        return;
    }
    let spans = occurrences
        .iter()
        .map(|occurrence| occurrence.span)
        .collect();
    let event_ids = occurrences
        .iter()
        .map(|occurrence| occurrence.event.0)
        .collect();
    evidence.push(ApiEvidence {
        kind,
        symbol,
        count: u32::try_from(occurrences.len()).unwrap_or(u32::MAX),
        spans,
        event_ids,
        related: Vec::new(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_common::BytePos;

    fn span(start: u32, end: u32) -> Span {
        Span::new(BytePos(start), BytePos(end))
    }

    #[test]
    fn typed_occurrence_index_is_sorted_and_deduplicated() {
        let mut index = OccurrenceIndex::<String>::default();
        index.push("fetch".into(), FactId(2), span(20, 26));
        index.push("fetch".into(), FactId(1), span(5, 11));
        index.push("fetch".into(), FactId(1), span(5, 11));
        index.normalize();
        assert_eq!(
            index
                .get("fetch")
                .unwrap()
                .iter()
                .map(|occurrence| occurrence.span)
                .collect::<Vec<_>>(),
            vec![span(5, 11), span(20, 26)]
        );
    }

    #[test]
    fn optimized_member_query_matches_reference_occurrences() {
        let mut facts = MatcherFacts::default();
        facts.record(ApiMatchKind::MemberCall, "client.request", span(30, 44));
        facts.record(ApiMatchKind::MemberCall, "other.request", span(5, 18));
        facts.record(ApiMatchKind::MemberCall, "client.request", span(10, 24));
        facts.normalize_occurrences();

        let matcher = ApiMatcher::from_matchers(vec![crate::api::rule::Matcher::member_call(
            MemberCallMatcher::syntactic_heuristic("client.request"),
        )]);
        let evidence = facts.evidence_for(&matcher);
        let reference = facts
            .member_calls
            .iter()
            .filter(|(symbol, _)| *symbol == "client.request")
            .flat_map(|(_, occurrences)| occurrences.iter().map(|occurrence| occurrence.span))
            .collect::<Vec<_>>();
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].spans, reference);
    }

    #[test]
    fn build_from_stream_populates_all_occurrence_indexes() {
        use super::super::facts::build::build_test_stream;
        use super::super::resolution::Resolver;

        let src = r#"
            import { foo } from 'mod';
            import { Bar } from 'other-mod';
            class MyClass extends Bar {}
            const x = foo;
            foo();
            x.hello();
            new MyClass();
            const s = "hello world";
            require('fs');
        "#;
        let parsed = crate::parse(src, "stream-index.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let stream = build_test_stream(&parsed.program, &resolver);

        let mut index = MatcherFacts::default();
        index.build_from_stream(&stream);
        index.normalize_occurrences();

        // Imports should have both 'mod' and 'other-mod' from import declarations,
        // and 'fs' from require() call.
        assert!(
            index.imports.get("mod").is_some(),
            "should have 'mod' import"
        );
        assert!(
            index.imports.get("other-mod").is_some(),
            "should have 'other-mod' import"
        );
        assert!(
            index.imports.get("fs").is_some(),
            "should have 'fs' require import"
        );

        // String literal should be indexed.
        assert!(
            index.string_literals.get("hello world").is_some(),
            "should have 'hello world' string literal"
        );

        // Class declaration should be indexed.
        assert!(
            index.classes.get("MyClass").is_some(),
            "should have MyClass class"
        );

        // Constructor call should be indexed.
        assert!(
            index.constructors.get("MyClass").is_some(),
            "should have MyClass constructor"
        );

        // foo() is an identifier call with module provenance.
        assert!(index.calls.get("foo").is_some(), "should have foo call");
        assert!(
            index
                .module_calls
                .get(&("mod".to_string(), "foo".to_string()))
                .is_some(),
            "should have foo as module call from 'mod'"
        );
    }
}
