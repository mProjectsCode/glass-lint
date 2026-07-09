use std::collections::BTreeMap;

use swc_common::Span;
use swc_ecma_ast::Program;

use super::result::{ApiEvidence, ApiMatchKind};
use super::rule::{
    ApiMatcher, ApiRule, CallMatcher, CallProvenance, ClassMatcher, ConstructorMatcher,
    FlowMatcher, MemberCallMatcher, MemberCallProvenance, MemberReadMatcher, MemberReadProvenance,
};

mod alias;
mod ast;
mod value_flow;
mod visitor;

pub use alias::AliasInfo;

type Occurrences = BTreeMap<String, Vec<Span>>;
type ModuleOccurrences = BTreeMap<(String, String), Vec<Span>>;

#[derive(Debug, Default)]
pub struct SymbolIndex {
    calls: Occurrences,
    global_calls: Occurrences,
    module_calls: ModuleOccurrences,
    member_calls: Occurrences,
    rooted_member_calls: Occurrences,
    module_member_calls: ModuleOccurrences,
    member_reads: Occurrences,
    rooted_member_reads: Occurrences,
    module_member_reads: ModuleOccurrences,
    imports: Occurrences,
    string_literals: Occurrences,
    classes: Occurrences,
    module_classes: ModuleOccurrences,
    constructors: Occurrences,
    global_constructors: Occurrences,
    module_constructors: ModuleOccurrences,
}

impl SymbolIndex {
    pub fn collect_for_rules(
        program: Option<&Program>,
        aliases: &AliasInfo,
        rules: &[ApiRule],
    ) -> (Self, Vec<Vec<ApiEvidence>>) {
        let member_matchers = rules
            .iter()
            .enumerate()
            .flat_map(|(rule_index, rule)| {
                ApiMatcher::from_matchers(rule.matchers.clone())
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
                ApiMatcher::from_matchers(rule.matchers.clone())
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
                ApiMatcher::from_matchers(rule.matchers.clone())
                    .flows
                    .into_iter()
                    .enumerate()
                    .map(move |(flow_index, matcher)| (rule_index, flow_index, matcher))
            })
            .collect::<Vec<_>>();
        Self::collect_with_argument_matchers(
            program,
            aliases,
            &member_matchers,
            &call_matchers,
            &flow_matchers,
            rules.len(),
        )
    }

    fn collect_with_argument_matchers(
        program: Option<&Program>,
        aliases: &AliasInfo,
        member_argument_matchers: &[(usize, MemberCallMatcher)],
        call_argument_matchers: &[(usize, CallMatcher)],
        flow_matchers: &[(usize, usize, FlowMatcher)],
        rule_count: usize,
    ) -> (Self, Vec<Vec<ApiEvidence>>) {
        let mut index = Self::default();
        let mut argument_evidence = vec![Vec::new(); rule_count];
        if let Some(program) = program {
            visitor::collect(
                program,
                aliases,
                member_argument_matchers,
                call_argument_matchers,
                &mut index,
                &mut argument_evidence,
            );
            let flow_evidence = value_flow::collect(program, aliases, flow_matchers, rule_count);
            for (rule_index, evidence) in flow_evidence.into_iter().enumerate() {
                argument_evidence[rule_index].extend(evidence);
            }
        }
        (index, argument_evidence)
    }

    pub fn evidence_for(&self, rule: &ApiRule) -> Vec<ApiEvidence> {
        let mut evidence = Vec::new();
        let matcher = ApiMatcher::from_matchers(rule.matchers.clone());
        self.collect_call_evidence(&matcher.calls, &mut evidence);
        self.collect_member_call_evidence(&matcher.member_calls, &mut evidence);
        self.collect_member_read_evidence(&matcher.member_reads, &mut evidence);
        self.collect_evidence(ApiMatchKind::Import, &matcher.imports, &mut evidence);
        self.collect_string_literal_evidence(&matcher.string_literals, &mut evidence);
        self.collect_class_evidence(&matcher.classes, &mut evidence);
        self.collect_constructor_evidence(&matcher.constructors, &mut evidence);

        evidence.truncate(ApiRule::EVIDENCE_LIMIT);
        evidence
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

    fn record(&mut self, kind: ApiMatchKind, symbol: impl Into<String>, span: Span) {
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
        count: spans.len() as u32,
        spans,
    });
}
