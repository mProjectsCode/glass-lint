//! ApiMatcher-specific occurrence queries and evidence construction.
//!
//! Queries select only the provenance index allowed by each matcher. They
//! return source-ordered evidence assembled from normalized occurrence slices;
//! argument-bearing matchers are delegated to the fact projections instead.

#[cfg(test)]
use swc_common::Span;

use super::{
    ApiEvidence, ApiMatchKind, ApiMatcher, CallMatcher, CallProvenance, ClassMatcher,
    ConstructorMatcher, MatcherFacts, MemberCallMatcher, MemberCallProvenance, MemberReadMatcher,
    MemberReadProvenance, push_evidence, push_owned_evidence,
};

impl MatcherFacts {
    /// Collect evidence for all matchers in one rule-independent index.
    ///
    /// The returned order follows matcher order and each occurrence bucket's
    /// deterministic fact/span order.
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
        matchers: &[crate::api::rule::ReturnedMemberCallMatcher],
        evidence: &mut Vec<ApiEvidence>,
    ) {
        for matcher in matchers {
            let spans = self
                .members
                .returned_calls
                .iter()
                .filter(|((source, member), _)| {
                    (source == &matcher.source
                        || source.starts_with(&format!("{}.", matcher.source)))
                        && member == &matcher.member
                })
                .flat_map(|(_, occurrences)| occurrences.iter().copied())
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
        matchers: &[crate::api::rule::ReturnedMemberReadMatcher],
        evidence: &mut Vec<ApiEvidence>,
    ) {
        for matcher in matchers {
            let spans = self
                .members
                .returned_reads
                .iter()
                .filter(|((source, member), _)| {
                    (source == &matcher.source
                        || source.starts_with(&format!("{}.", matcher.source)))
                        && member == &matcher.member
                })
                .flat_map(|(_, occurrences)| occurrences.iter().copied())
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
        matchers: &[crate::api::rule::InstanceMemberCallMatcher],
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
                self.members.instance_calls.get(&key).cloned(),
            );
        }
    }

    fn collect_call_evidence(&self, calls: &[CallMatcher], evidence: &mut Vec<ApiEvidence>) {
        for call in calls {
            if !call.arguments.is_empty() {
                continue;
            }
            let spans = match &call.provenance {
                CallProvenance::Any => self.call_indexes.calls.get(&call.name),
                CallProvenance::Global => self.call_indexes.global_calls.get(&call.name),
                CallProvenance::ModuleExport { module } => self
                    .call_indexes
                    .module_calls
                    .get(&(module.clone(), call.name.clone())),
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
                        self.members.reads.get(&read.chain).cloned()
                    } else {
                        let suffix = format!(".{}", read.chain);
                        let spans = self
                            .members
                            .reads
                            .iter()
                            .filter(|(member_read, _)| {
                                *member_read == &read.chain || member_read.ends_with(&suffix)
                            })
                            .flat_map(|(_, occurrences)| occurrences.iter().copied())
                            .collect::<Vec<_>>();
                        (!spans.is_empty()).then_some(spans)
                    }
                }
                MemberReadProvenance::Rooted => self.members.rooted_reads.get(&read.chain).cloned(),
                MemberReadProvenance::ModuleNamespace { module } => self
                    .members
                    .module_reads
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
            if !call.arguments.is_empty() {
                continue;
            }
            let spans = match &call.provenance {
                MemberCallProvenance::Any => self.members.calls.get(&call.chain),
                MemberCallProvenance::Rooted => self.members.rooted_calls.get(&call.chain),
                MemberCallProvenance::ModuleNamespace { module } => self
                    .members
                    .module_calls
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
                CallProvenance::Any | CallProvenance::Global => {
                    self.constructions.classes.get(&class.name)
                }
                CallProvenance::ModuleExport { module } => self
                    .constructions
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
                CallProvenance::Any | CallProvenance::Global => self
                    .constructions
                    .global_constructors
                    .get(&constructor.name),
                CallProvenance::ModuleExport { module } => self
                    .constructions
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
                ApiMatchKind::Import => self.literals.imports.get(symbol),
                _ => None,
            };
            push_evidence(evidence, kind, symbol.clone(), spans);
        }
    }

    fn collect_string_literal_evidence(&self, markers: &[String], evidence: &mut Vec<ApiEvidence>) {
        for marker in markers {
            let spans = self
                .literals
                .strings
                .iter()
                .filter(|(literal, _)| literal.contains(marker))
                .flat_map(|(_, occurrences)| occurrences.iter().copied())
                .collect::<Vec<_>>();
            push_owned_evidence(
                evidence,
                ApiMatchKind::StringLiteral,
                marker.clone(),
                (!spans.is_empty()).then_some(spans),
            );
        }
    }

    #[cfg(test)]
    pub(super) fn record(&mut self, kind: ApiMatchKind, symbol: impl Into<String>, span: Span) {
        use crate::analysis::facts::FactId;

        let symbol = symbol.into();
        match kind {
            ApiMatchKind::Call => {
                self.call_indexes.calls.push(symbol, FactId(u32::MAX), span);
            }
            ApiMatchKind::MemberCall => {
                self.members.calls.push(symbol, FactId(u32::MAX), span);
            }
            ApiMatchKind::MemberRead => {
                self.members.reads.push(symbol, FactId(u32::MAX), span);
            }
            ApiMatchKind::Import => {
                self.literals.imports.push(symbol, FactId(u32::MAX), span);
            }
            ApiMatchKind::StringLiteral => {
                self.literals.strings.push(symbol, FactId(u32::MAX), span);
            }
            ApiMatchKind::Class => {
                self.constructions
                    .classes
                    .push(symbol, FactId(u32::MAX), span);
            }
            ApiMatchKind::Constructor => {
                self.constructions
                    .constructors
                    .push(symbol, FactId(u32::MAX), span);
            }
            ApiMatchKind::CallArgument => {}
        }
    }
}
