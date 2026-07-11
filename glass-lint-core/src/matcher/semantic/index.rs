//! Rule-independent indexes built from one semantic model.
//!
//! Collection is intentionally separated from `evidence_for`: the AST is
//! walked once, then each rule selects from deterministic occurrence maps.
//! Argument and flow evidence remain per-rule because their matchers carry
//! rule-specific predicates that cannot be represented as a shared key.

use std::collections::BTreeMap;

use swc_common::Span;
use swc_ecma_ast::Program;

use super::super::result::{ApiEvidence, ApiMatchKind};
use super::super::rule::{
    ApiMatcher, ApiRule, CallMatcher, CallProvenance, ClassMatcher, ConstructorMatcher,
    FlowMatcher, MemberCallMatcher, MemberCallProvenance, MemberReadMatcher, MemberReadProvenance,
};

use super::resolver::Resolver;

type Occurrences = BTreeMap<String, Vec<Span>>;
type ModuleOccurrences = BTreeMap<(String, String), Vec<Span>>;

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
    pub(super) returned_member_calls: BTreeMap<(String, String), Vec<Span>>,
    pub(super) returned_member_reads: BTreeMap<(String, String), Vec<Span>>,
    pub(super) instance_member_calls: BTreeMap<(String, String, String), Vec<Span>>,
    pub(super) imports: Occurrences,
    pub(super) string_literals: Occurrences,
    pub(super) classes: Occurrences,
    pub(super) module_classes: ModuleOccurrences,
    pub(super) constructors: Occurrences,
    pub(super) global_constructors: Occurrences,
    pub(super) module_constructors: ModuleOccurrences,
}

impl MatcherFacts {
    pub fn collect_for_rules(
        program: &Program,
        resolver: &Resolver,
        rules: &[ApiRule],
    ) -> (Self, Vec<Vec<ApiEvidence>>) {
        // Pre-filtering avoids evaluating argument predicates during evidence
        // lookup for matchers that only care about a callee occurrence.
        let member_matchers = rules
            .iter()
            .enumerate()
            .flat_map(|(rule_index, rule)| {
                ApiMatcher::from_matchers(rule.matchers().to_vec())
                    .member_calls
                    .into_iter()
                    .filter(|matcher| {
                        !matcher.arg_strings.is_empty()
                            || !matcher.arg_object_keys.is_empty()
                            || !matcher.arg_rooted_exprs.is_empty()
                    })
                    .map(move |matcher| (rule_index, matcher))
            })
            .collect::<Vec<_>>();
        let call_matchers = rules
            .iter()
            .enumerate()
            .flat_map(|(rule_index, rule)| {
                ApiMatcher::from_matchers(rule.matchers().to_vec())
                    .calls
                    .into_iter()
                    .filter(|matcher| !matcher.arg_strings.is_empty())
                    .map(move |matcher| (rule_index, matcher))
            })
            .collect::<Vec<_>>();
        let flow_matchers = rules
            .iter()
            .enumerate()
            .flat_map(|(rule_index, rule)| {
                ApiMatcher::from_matchers(rule.matchers().to_vec())
                    .flows
                    .into_iter()
                    .enumerate()
                    .map(move |(flow_index, matcher)| (rule_index, flow_index, matcher))
            })
            .collect::<Vec<_>>();
        let instance_matchers = rules
            .iter()
            .flat_map(|rule| {
                ApiMatcher::from_matchers(rule.matchers().to_vec()).instance_member_calls
            })
            .collect::<Vec<_>>();
        Self::collect_with_argument_matchers(
            program,
            resolver,
            &member_matchers,
            &call_matchers,
            &flow_matchers,
            &instance_matchers,
            rules.len(),
        )
    }

    fn collect_with_argument_matchers(
        program: &Program,
        resolver: &Resolver,
        member_argument_matchers: &[(usize, MemberCallMatcher)],
        call_argument_matchers: &[(usize, CallMatcher)],
        flow_matchers: &[(usize, usize, FlowMatcher)],
        instance_matchers: &[super::super::rule::InstanceMemberCallMatcher],
        rule_count: usize,
    ) -> (Self, Vec<Vec<ApiEvidence>>) {
        let mut index = Self::default();
        let mut argument_evidence = vec![Vec::new(); rule_count];
        super::calls::collect(
            program,
            resolver,
            member_argument_matchers,
            call_argument_matchers,
            &mut index,
            &mut argument_evidence,
        );
        super::instance::collect(program, resolver, instance_matchers, &mut index);
        let flow_evidence =
            super::object_flow::collect(program, resolver, flow_matchers, rule_count);
        for (rule_index, evidence) in flow_evidence.into_iter().enumerate() {
            argument_evidence[rule_index].extend(evidence);
        }
        index.normalize_occurrences();
        (index, argument_evidence)
    }

    fn normalize_occurrences(&mut self) {
        sort_spans(&mut self.calls);
        sort_spans(&mut self.global_calls);
        sort_spans(&mut self.module_calls);
        sort_spans(&mut self.member_calls);
        sort_spans(&mut self.rooted_member_calls);
        sort_spans(&mut self.module_member_calls);
        sort_spans(&mut self.member_reads);
        sort_spans(&mut self.rooted_member_reads);
        sort_spans(&mut self.module_member_reads);
        sort_spans(&mut self.returned_member_calls);
        sort_spans(&mut self.returned_member_reads);
        sort_spans(&mut self.instance_member_calls);
        sort_spans(&mut self.imports);
        sort_spans(&mut self.string_literals);
        sort_spans(&mut self.classes);
        sort_spans(&mut self.module_classes);
        sort_spans(&mut self.constructors);
        sort_spans(&mut self.global_constructors);
        sort_spans(&mut self.module_constructors);
    }

    pub fn evidence_for(&self, rule: &ApiRule) -> Vec<ApiEvidence> {
        let mut evidence = Vec::new();
        let matcher = ApiMatcher::from_matchers(rule.matchers().to_vec());
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
        let target = match kind {
            ApiMatchKind::Call => &mut self.calls,
            ApiMatchKind::MemberCall => &mut self.member_calls,
            ApiMatchKind::MemberRead => &mut self.member_reads,
            ApiMatchKind::Import => &mut self.imports,
            ApiMatchKind::StringLiteral => &mut self.string_literals,
            ApiMatchKind::Class => &mut self.classes,
            ApiMatchKind::Constructor => &mut self.constructors,
            ApiMatchKind::CallArgument => return,
        };

        target.entry(symbol.into()).or_default().push(span);
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

fn sort_spans<K: Ord>(occurrences: &mut BTreeMap<K, Vec<Span>>) {
    for spans in occurrences.values_mut() {
        spans.sort_by_key(|span| (span.lo, span.hi));
        spans.dedup();
    }
}
