//! Rule-independent indexes built from one semantic model.
//!
//! Collection is intentionally separated from `evidence_for`: the AST is
//! walked once, then each rule selects from deterministic occurrence maps.
//! Argument and flow evidence remain per-rule because their matchers carry
//! rule-specific predicates that cannot be represented as a shared key.

use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
};

use swc_common::Span;

use super::super::result::{ApiEvidence, ApiMatchKind};
use super::super::rule::{
    ApiMatcher, CallMatcher, CallProvenance, ClassMatcher, ConstructorMatcher, MemberCallMatcher,
    MemberCallProvenance, MemberReadMatcher, MemberReadProvenance,
};

/// Typed occurrence storage.  Keeping insertion and normalization in one
/// container prevents semantic collectors from inventing subtly different
/// span ordering or duplicate policies for each provenance view.
#[derive(Debug, Default)]
pub(super) struct OccurrenceIndex<K: Ord>(BTreeMap<K, Vec<Span>>);

impl<K: Ord> OccurrenceIndex<K> {
    pub(super) fn push(&mut self, key: K, span: Span) {
        self.0.entry(key).or_default().push(span);
    }

    pub(super) fn normalize(&mut self) {
        for spans in self.0.values_mut() {
            spans.sort_by_key(|span| (span.lo, span.hi));
            spans.dedup();
        }
    }
}

impl<K: Ord> Deref for OccurrenceIndex<K> {
    type Target = BTreeMap<K, Vec<Span>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<K: Ord> DerefMut for OccurrenceIndex<K> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

type Occurrences = OccurrenceIndex<String>;
type ModuleOccurrences = OccurrenceIndex<(String, String)>;

#[derive(Debug, Default)]
pub struct MatcherFacts {
    // Each map represents a different confidence/provenance level. Do not
    // collapse these into one index: a global spelling, rooted alias, and
    // imported member have intentionally different matching semantics.
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

impl MatcherFacts {
    pub(super) fn normalize_occurrences(&mut self) {
        self.calls.normalize();
        self.global_calls.normalize();
        self.module_calls.normalize();
        self.member_calls.normalize();
        self.rooted_member_calls.normalize();
        self.module_member_calls.normalize();
        self.member_reads.normalize();
        self.rooted_member_reads.normalize();
        self.module_member_reads.normalize();
        self.returned_member_calls.normalize();
        self.returned_member_reads.normalize();
        self.instance_member_calls.normalize();
        self.imports.normalize();
        self.string_literals.normalize();
        self.classes.normalize();
        self.module_classes.normalize();
        self.constructors.normalize();
        self.global_constructors.normalize();
        self.module_constructors.normalize();
    }

    pub fn evidence_for(&self, matcher: &ApiMatcher) -> Vec<ApiEvidence> {
        let mut evidence = Vec::new();
        self.collect_call_evidence(&matcher.calls, &mut evidence);
        self.collect_member_call_evidence(&matcher.member_calls, &mut evidence);
        self.collect_member_read_evidence(&matcher.member_reads, &mut evidence);
        self.collect_evidence(ApiMatchKind::Import, &matcher.imports, &mut evidence);
        self.collect_string_literal_evidence(&matcher.string_literals, &mut evidence);
        self.collect_class_evidence(&matcher.classes, &mut evidence);
        self.collect_constructor_evidence(&matcher.constructors, &mut evidence);
        self.collect_returned_member_call_evidence(&matcher.returned_member_calls, &mut evidence);
        self.collect_returned_member_read_evidence(&matcher.returned_member_reads, &mut evidence);
        self.collect_instance_member_call_evidence(&matcher.instance_member_calls, &mut evidence);

        evidence
    }

    fn collect_returned_member_call_evidence(
        &self,
        matchers: &[super::super::rule::ReturnedMemberCallMatcher],
        evidence: &mut Vec<ApiEvidence>,
    ) {
        for matcher in matchers {
            let spans = self
                .returned_member_calls
                .iter()
                .filter(|((source, member), _)| {
                    (source == &matcher.source
                        || source.starts_with(&format!("{}.", matcher.source)))
                        && member == &matcher.member
                })
                .flat_map(|(_, spans)| spans.iter().copied())
                .collect::<Vec<_>>();
            push_owned_evidence(
                evidence,
                ApiMatchKind::MemberCall,
                format!("{}.{}", matcher.source, matcher.member),
                (!spans.is_empty()).then_some(spans),
            );
        }
    }

    fn collect_returned_member_read_evidence(
        &self,
        matchers: &[super::super::rule::ReturnedMemberReadMatcher],
        evidence: &mut Vec<ApiEvidence>,
    ) {
        for matcher in matchers {
            let spans = self
                .returned_member_reads
                .iter()
                .filter(|((source, member), _)| {
                    (source == &matcher.source
                        || source.starts_with(&format!("{}.", matcher.source)))
                        && member == &matcher.member
                })
                .flat_map(|(_, spans)| spans.iter().copied())
                .collect::<Vec<_>>();
            push_owned_evidence(
                evidence,
                ApiMatchKind::MemberRead,
                format!("{}.{}", matcher.source, matcher.member),
                (!spans.is_empty()).then_some(spans),
            );
        }
    }

    fn collect_instance_member_call_evidence(
        &self,
        matchers: &[super::super::rule::InstanceMemberCallMatcher],
        evidence: &mut Vec<ApiEvidence>,
    ) {
        for matcher in matchers {
            let key = (
                matcher.module.clone(),
                matcher.export.clone(),
                matcher.member.clone(),
            );
            push_owned_evidence(
                evidence,
                ApiMatchKind::MemberCall,
                format!("{}:{}.{}", matcher.module, matcher.export, matcher.member),
                self.instance_member_calls.get(&key).cloned(),
            );
        }
    }

    fn collect_call_evidence(&self, calls: &[CallMatcher], evidence: &mut Vec<ApiEvidence>) {
        for call in calls {
            if !call.arg_strings.is_empty() {
                continue;
            }
            let spans = match &call.provenance {
                CallProvenance::Any => self.calls.get(&call.name),
                CallProvenance::Global => self.global_calls.get(&call.name),
                CallProvenance::ModuleExport { module } => {
                    self.module_calls.get(&(module.clone(), call.name.clone()))
                }
            };
            push_evidence(evidence, ApiMatchKind::Call, call.evidence_symbol(), spans);
        }
    }

    fn collect_member_read_evidence(
        &self,
        member_reads: &[MemberReadMatcher],
        evidence: &mut Vec<ApiEvidence>,
    ) {
        for read in member_reads {
            let spans = match &read.provenance {
                MemberReadProvenance::Any => {
                    if read.chain.contains('.') {
                        self.member_reads.get(&read.chain).cloned()
                    } else {
                        let suffix = format!(".{}", read.chain);
                        let spans = self
                            .member_reads
                            .iter()
                            .filter(|(member_read, _)| {
                                *member_read == &read.chain || member_read.ends_with(&suffix)
                            })
                            .flat_map(|(_, spans)| spans.iter().copied())
                            .collect::<Vec<_>>();
                        (!spans.is_empty()).then_some(spans)
                    }
                }
                MemberReadProvenance::Rooted => self.rooted_member_reads.get(&read.chain).cloned(),
                MemberReadProvenance::ModuleNamespace { module } => self
                    .module_member_reads
                    .get(&(module.clone(), read.chain.clone()))
                    .cloned(),
            };
            push_owned_evidence(
                evidence,
                ApiMatchKind::MemberRead,
                read.evidence_symbol(),
                spans,
            );
        }
    }

    fn collect_member_call_evidence(
        &self,
        member_calls: &[MemberCallMatcher],
        evidence: &mut Vec<ApiEvidence>,
    ) {
        for call in member_calls {
            if !call.arg_strings.is_empty()
                || !call.arg_object_keys.is_empty()
                || !call.arg_rooted_exprs.is_empty()
            {
                continue;
            }
            let spans = match &call.provenance {
                MemberCallProvenance::Any => self.member_calls.get(&call.chain),
                MemberCallProvenance::Rooted => self.rooted_member_calls.get(&call.chain),
                MemberCallProvenance::ModuleNamespace { module } => self
                    .module_member_calls
                    .get(&(module.clone(), call.chain.clone())),
            };
            push_evidence(
                evidence,
                ApiMatchKind::MemberCall,
                call.evidence_symbol(),
                spans,
            );
        }
    }

    fn collect_class_evidence(&self, classes: &[ClassMatcher], evidence: &mut Vec<ApiEvidence>) {
        for class in classes {
            let spans = match &class.provenance {
                CallProvenance::Any | CallProvenance::Global => self.classes.get(&class.name),
                CallProvenance::ModuleExport { module } => self
                    .module_classes
                    .get(&(module.clone(), class.name.clone())),
            };
            push_evidence(
                evidence,
                ApiMatchKind::Class,
                class.evidence_symbol(),
                spans,
            );
        }
    }

    fn collect_constructor_evidence(
        &self,
        constructors: &[ConstructorMatcher],
        evidence: &mut Vec<ApiEvidence>,
    ) {
        for constructor in constructors {
            let spans = match &constructor.provenance {
                CallProvenance::Any => self.constructors.get(&constructor.name),
                CallProvenance::Global => self.global_constructors.get(&constructor.name),
                CallProvenance::ModuleExport { module } => self
                    .module_constructors
                    .get(&(module.clone(), constructor.name.clone())),
            };
            push_evidence(
                evidence,
                ApiMatchKind::Constructor,
                constructor.evidence_symbol(),
                spans,
            );
        }
    }

    fn collect_evidence(
        &self,
        kind: ApiMatchKind,
        symbols: &[String],
        evidence: &mut Vec<ApiEvidence>,
    ) {
        for symbol in symbols {
            let spans = match kind {
                ApiMatchKind::Import => self.imports.get(symbol),
                _ => None,
            };
            push_evidence(evidence, kind, symbol.clone(), spans);
        }
    }

    fn collect_string_literal_evidence(&self, markers: &[String], evidence: &mut Vec<ApiEvidence>) {
        for marker in markers {
            let spans = self
                .string_literals
                .iter()
                .filter(|(literal, _)| literal.contains(marker))
                .flat_map(|(_, spans)| spans.iter().copied())
                .collect::<Vec<_>>();
            push_owned_evidence(
                evidence,
                ApiMatchKind::StringLiteral,
                marker.clone(),
                (!spans.is_empty()).then_some(spans),
            );
        }
    }

    pub(super) fn record(&mut self, kind: ApiMatchKind, symbol: impl Into<String>, span: Span) {
        let symbol = symbol.into();
        match kind {
            ApiMatchKind::Call => {
                self.calls.push(symbol, span);
            }
            ApiMatchKind::MemberCall => {
                self.member_calls.push(symbol, span);
            }
            ApiMatchKind::MemberRead => {
                self.member_reads.push(symbol, span);
            }
            ApiMatchKind::Import => {
                self.imports.push(symbol, span);
            }
            ApiMatchKind::StringLiteral => {
                self.string_literals.push(symbol, span);
            }
            ApiMatchKind::Class => {
                self.classes.push(symbol, span);
            }
            ApiMatchKind::Constructor => {
                self.constructors.push(symbol, span);
            }
            ApiMatchKind::CallArgument => {}
        }
    }
}

fn push_evidence(
    evidence: &mut Vec<ApiEvidence>,
    kind: ApiMatchKind,
    symbol: String,
    spans: Option<&Vec<Span>>,
) {
    push_owned_evidence(evidence, kind, symbol, spans.cloned());
}

fn push_owned_evidence(
    evidence: &mut Vec<ApiEvidence>,
    kind: ApiMatchKind,
    symbol: String,
    spans: Option<Vec<Span>>,
) {
    let Some(spans) = spans else {
        return;
    };
    if spans.is_empty() {
        return;
    }
    evidence.push(ApiEvidence {
        kind,
        symbol,
        count: u32::try_from(spans.len()).unwrap_or(u32::MAX),
        spans,
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
        index.push("fetch".into(), span(20, 26));
        index.push("fetch".into(), span(5, 11));
        index.push("fetch".into(), span(5, 11));
        index.normalize();
        assert_eq!(index.get("fetch").unwrap(), &[span(5, 11), span(20, 26)]);
    }

    #[test]
    fn optimized_member_query_matches_reference_occurrences() {
        let mut facts = MatcherFacts::default();
        facts.record(ApiMatchKind::MemberCall, "client.request", span(30, 44));
        facts.record(ApiMatchKind::MemberCall, "other.request", span(5, 18));
        facts.record(ApiMatchKind::MemberCall, "client.request", span(10, 24));
        facts.normalize_occurrences();

        let matcher =
            ApiMatcher::from_matchers(vec![super::super::super::rule::Matcher::member_call(
                MemberCallMatcher::syntactic_heuristic("client.request"),
            )]);
        let evidence = facts.evidence_for(&matcher);
        let reference = facts
            .member_calls
            .iter()
            .filter(|(symbol, _)| *symbol == "client.request")
            .flat_map(|(_, spans)| spans.iter().copied())
            .collect::<Vec<_>>();
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].spans, reference);
    }
}
