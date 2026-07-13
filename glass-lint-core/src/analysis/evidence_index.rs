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

use super::ast::{SymbolCallProvenance, SymbolMemberProvenance};
use super::facts::{CallArgInfo, FactId, FactPayload, FactStream};
use crate::api::classification::{ApiEvidence, ApiMatchKind};
use crate::api::rule::{
    ApiMatcher, CallMatcher, CallProvenance, ClassMatcher, ConstructorMatcher, MemberCallMatcher,
    MemberCallProvenance, MemberReadMatcher, MemberReadProvenance, canonical_rooted_chain,
};

/// Typed occurrence storage. Keeping insertion and normalization in one
/// container prevents semantic collectors from inventing subtly different
/// span ordering or duplicate policies for each provenance view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Occurrence {
    pub(super) event: FactId,
    pub(super) span: Span,
}

#[derive(Debug, Default)]
pub(super) struct OccurrenceIndex<K: Ord>(BTreeMap<K, Vec<Occurrence>>);

impl<K: Ord> OccurrenceIndex<K> {
    pub(super) fn push(&mut self, key: K, event: FactId, span: Span) {
        self.0
            .entry(key)
            .or_default()
            .push(Occurrence { event, span });
    }

    pub(super) fn normalize(&mut self) {
        for occurrences in self.0.values_mut() {
            occurrences.sort_by_key(|occurrence| {
                (occurrence.event, occurrence.span.lo, occurrence.span.hi)
            });
            occurrences.dedup();
        }
    }
}

impl<K: Ord> Deref for OccurrenceIndex<K> {
    type Target = BTreeMap<K, Vec<Occurrence>>;

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

    /// Project a rule-independent `FactStream` into occurrence indexes.
    ///
    /// Every fact kind is projected exactly once into
    /// the matching occurrence map based on its provenance fields.
    pub(super) fn build_from_stream(&mut self, stream: &FactStream) {
        for fact in stream.facts() {
            match &fact.payload {
                FactPayload::Call {
                    callee_name,
                    callee_span,
                    call_provenance,
                    syntactic_chain,
                    rooted_chain,
                    module_member,
                    returned_member,
                    instance_class,
                    unwrap,
                    ..
                } => {
                    // Use callee_span (member/ident span) for occurrences
                    // rather than the full call expression span.
                    let span = *callee_span;

                    // Syntactic name for identifier calls.
                    if let Some(name) = callee_name {
                        self.calls.push(name.clone(), fact.id, span);
                    }

                    // Provenance-based call indexes.
                    match call_provenance {
                        SymbolCallProvenance::Global { name } => {
                            self.global_calls.push(name.clone(), fact.id, span);
                        }
                        SymbolCallProvenance::ModuleExport { module, export } => {
                            self.module_calls
                                .push((module.clone(), export.clone()), fact.id, span);
                            self.module_member_calls.push(
                                (module.clone(), export.clone()),
                                fact.id,
                                span,
                            );
                        }
                        SymbolCallProvenance::Local => {}
                    }

                    // Member call indexes for member-expression callees.
                    if let Some(chain) = syntactic_chain {
                        self.member_calls.push(chain.clone(), fact.id, span);
                    }
                    if let Some(chain) = rooted_chain {
                        self.rooted_member_calls.push(
                            canonical_rooted_chain(chain).to_string(),
                            fact.id,
                            span,
                        );
                    }

                    // Module namespace provenance from member expression.
                    if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) =
                        module_member
                    {
                        self.module_calls
                            .push((module.clone(), member.clone()), fact.id, span);
                        self.module_member_calls.push(
                            (module.clone(), member.clone()),
                            fact.id,
                            span,
                        );
                    }

                    // Returned member from function return types.
                    if let Some((source, member)) = returned_member {
                        self.returned_member_calls.push(
                            (source.clone(), member.clone()),
                            fact.id,
                            span,
                        );
                    }

                    // Instance member call: this.method() inside a class
                    // with a known module superclass.
                    if let Some((module, export)) = instance_class
                        && let Some(member_name) = syntactic_chain
                            .as_ref()
                            .and_then(|chain| chain.rsplit('.').next())
                    {
                        self.instance_member_calls.push(
                            (module.clone(), export.clone(), member_name.to_string()),
                            fact.id,
                            span,
                        );
                    }

                    // Special case: `Function` constructor calls via member
                    // expression (e.g., `(0, Function)(code)`).
                    if rooted_chain.as_deref() == Some("Function") {
                        self.global_calls
                            .push("Function".to_string(), fact.id, span);
                        self.calls.push("Function".to_string(), fact.id, span);
                    }

                    // .call()/.apply() unwrapping: also record the target
                    // as a member call so argument predicates can match
                    // against the effective arguments.
                    if let Some(unwrap) = unwrap
                        && !unwrap.chain.is_empty()
                    {
                        self.member_calls.push(unwrap.chain.clone(), fact.id, span);
                        self.rooted_member_calls.push(
                            canonical_rooted_chain(&unwrap.chain).to_string(),
                            fact.id,
                            span,
                        );
                    }
                }

                FactPayload::MemberRead {
                    syntactic_chain,
                    rooted_chain,
                    module_member,
                    returned_member,
                    ..
                } => {
                    if let Some(chain) = syntactic_chain {
                        self.member_reads.push(chain.clone(), fact.id, fact.span);
                    }
                    if let Some(chain) = rooted_chain {
                        self.rooted_member_reads.push(
                            canonical_rooted_chain(chain).to_string(),
                            fact.id,
                            fact.span,
                        );
                    }
                    if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) =
                        module_member
                    {
                        self.module_member_reads.push(
                            (module.clone(), member.clone()),
                            fact.id,
                            fact.span,
                        );
                        // Record as a class occurrence for module namespace
                        // A module member read is also a class occurrence for
                        // the module-class matcher.
                        self.classes.push(member.clone(), fact.id, fact.span);
                    }
                    if let Some((source, member)) = returned_member {
                        self.returned_member_reads.push(
                            (source.clone(), member.clone()),
                            fact.id,
                            fact.span,
                        );
                    }
                }

                FactPayload::Construction {
                    callee_name,
                    callee_span,
                    provenance,
                    ..
                } => {
                    let span = *callee_span;
                    if let Some(name) = callee_name {
                        self.constructors.push(name.clone(), fact.id, span);
                    }
                    match provenance {
                        SymbolCallProvenance::Global { name } => {
                            self.global_constructors.push(name.clone(), fact.id, span);
                        }
                        SymbolCallProvenance::ModuleExport { module, export } => {
                            self.module_constructors.push(
                                (module.clone(), export.clone()),
                                fact.id,
                                span,
                            );
                        }
                        SymbolCallProvenance::Local => {}
                    }
                }

                FactPayload::Import { module } => {
                    self.imports.push(module.clone(), fact.id, fact.span);
                }

                FactPayload::Reference {
                    static_string: Some(value),
                    ..
                } => {
                    self.string_literals.push(value.clone(), fact.id, fact.span);
                }

                FactPayload::Class { name, provenance } => {
                    if !name.is_empty() {
                        self.classes.push(name.clone(), fact.id, fact.span);
                    }
                    if let Some((module, export)) = provenance {
                        self.module_classes.push(
                            (module.clone(), export.clone()),
                            fact.id,
                            fact.span,
                        );
                    }
                }

                // Declaration, Assignment, PropertyWrite, Reference facts
                // do not contribute to occurrence indexes.
                FactPayload::Declaration { .. }
                | FactPayload::Assignment { .. }
                | FactPayload::PropertyWrite { .. }
                | FactPayload::Reference {
                    static_string: None,
                    ..
                }
                | FactPayload::Function { .. }
                | FactPayload::Control { .. } => {}
            }
        }
    }

    /// Compute argument predicate evidence directly from the `FactStream`.
    ///
    /// For each `Call` fact, check call-argument and member-argument matchers
    /// against the pre-computed argument info.  For `.call()`/`.apply()` calls,
    /// the effective arguments after unwrapping are used for call-argument
    /// matching.
    pub(super) fn compute_argument_evidence_from_stream(
        &self,
        stream: &FactStream,
        member_argument_matchers: &[(usize, &MemberCallMatcher)],
        call_argument_matchers: &[(usize, &CallMatcher)],
        argument_evidence: &mut [Vec<ApiEvidence>],
    ) {
        for fact in stream.facts() {
            if let FactPayload::Call {
                callee_name,
                call_provenance,
                syntactic_chain,
                rooted_chain,
                module_member,
                args,
                unwrap,
                ..
            } = &fact.payload
            {
                // Member argument predicates use the original args.
                if !member_argument_matchers.is_empty() {
                    self.collect_member_argument_evidence_from_args(
                        member_argument_matchers,
                        argument_evidence,
                        fact.id,
                        fact.span,
                        syntactic_chain.as_deref(),
                        rooted_chain.as_deref(),
                        module_member.as_ref(),
                        args,
                    );
                }

                // Call argument predicates: for .call()/.apply(), use the
                // effective args after unwrapping.
                if !call_argument_matchers.is_empty() {
                    let (effective_args, effective_name, effective_provenance) =
                        if let Some(u) = unwrap {
                            (&u.effective_args, Some(u.chain.as_str()), call_provenance)
                        } else {
                            (args, callee_name.as_deref(), call_provenance)
                        };
                    self.collect_call_argument_evidence_from_args(
                        call_argument_matchers,
                        argument_evidence,
                        fact.id,
                        fact.span,
                        effective_name,
                        effective_provenance,
                        effective_args,
                    );
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_call_argument_evidence_from_args(
        &self,
        matchers: &[(usize, &CallMatcher)],
        evidence: &mut [Vec<ApiEvidence>],
        event: FactId,
        span: Span,
        callee_name: Option<&str>,
        call_provenance: &SymbolCallProvenance,
        args: &[CallArgInfo],
    ) {
        for (rule_index, matcher) in matchers {
            let matcher = *matcher;
            let call_matches = match &matcher.provenance {
                CallProvenance::Any => {
                    callee_name.is_some_and(|name| name == matcher.name.as_str())
                }
                CallProvenance::Global => matches!(
                    call_provenance,
                    SymbolCallProvenance::Global { name } if name == &matcher.name
                ),
                CallProvenance::ModuleExport { module } => matches!(
                    call_provenance,
                    SymbolCallProvenance::ModuleExport {
                        module: found_module,
                        export
                    } if found_module == module && export == &matcher.name
                ),
            };

            if call_matches
                && matcher.arg_strings.iter().all(|arg_matcher| {
                    args.get(arg_matcher.index).is_some_and(|arg| {
                        arg.static_string.as_ref().is_some_and(|value| {
                            arg_matcher.predicate.as_ref().map_or_else(
                                || {
                                    arg_matcher.values.is_empty()
                                        || arg_matcher.values.iter().any(|e| e == value)
                                },
                                |predicate| {
                                    super::flow_calls::matches_static_value(predicate, value)
                                },
                            )
                        })
                    })
                })
            {
                evidence[*rule_index].push(ApiEvidence {
                    kind: ApiMatchKind::CallArgument,
                    symbol: matcher.evidence_symbol(),
                    count: 1,
                    spans: vec![span],
                    event_ids: vec![event.0],
                });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_member_argument_evidence_from_args(
        &self,
        matchers: &[(usize, &MemberCallMatcher)],
        evidence: &mut [Vec<ApiEvidence>],
        event: FactId,
        span: Span,
        syntactic_chain: Option<&str>,
        resolved_chain: Option<&str>,
        module_member: Option<&SymbolMemberProvenance>,
        args: &[CallArgInfo],
    ) {
        for (rule_index, matcher) in matchers {
            let matcher = *matcher;
            let member_matches = match &matcher.provenance {
                MemberCallProvenance::Any => {
                    syntactic_chain == Some(&matcher.chain)
                        || resolved_chain == Some(&matcher.chain)
                }
                MemberCallProvenance::Rooted => resolved_chain
                    .map(canonical_rooted_chain)
                    .is_some_and(|chain| chain == matcher.chain),
                MemberCallProvenance::ModuleNamespace { module } => matches!(
                    module_member,
                    Some(SymbolMemberProvenance::ModuleNamespace {
                        module: found_module,
                        member
                    }) if found_module == module && member == &matcher.chain
                ),
            };
            if member_matches
                && matcher.arg_strings.iter().all(|arg_matcher| {
                    args.get(arg_matcher.index).is_some_and(|arg| {
                        arg.static_string.as_ref().is_some_and(|value| {
                            arg_matcher.predicate.as_ref().map_or_else(
                                || {
                                    arg_matcher.values.is_empty()
                                        || arg_matcher.values.iter().any(|e| e == value)
                                },
                                |predicate| {
                                    super::flow_calls::matches_static_value(predicate, value)
                                },
                            )
                        })
                    })
                })
                && matcher.arg_object_keys.iter().all(|key_matcher| {
                    args.get(key_matcher.index)
                        .and_then(|arg| arg.object_keys.as_ref())
                        .is_some_and(|keys| {
                            key_matcher
                                .keys
                                .iter()
                                .all(|expected| keys.iter().any(|key| key == expected))
                        })
                })
                && matcher.arg_rooted_exprs.iter().all(|root_matcher| {
                    args.get(root_matcher.index)
                        .and_then(|arg| arg.rooted_chain.as_ref())
                        .map(|chain| canonical_rooted_chain(chain).to_string())
                        .is_some_and(|chain| {
                            root_matcher
                                .chains
                                .iter()
                                .any(|expected| expected == &chain)
                        })
                })
            {
                let symbol = match matcher.provenance {
                    MemberCallProvenance::Any => matcher.evidence_symbol(),
                    MemberCallProvenance::Rooted | MemberCallProvenance::ModuleNamespace { .. } => {
                        syntactic_chain.unwrap_or(&matcher.chain).to_string()
                    }
                };
                evidence[*rule_index].push(ApiEvidence {
                    kind: ApiMatchKind::CallArgument,
                    symbol,
                    count: 1,
                    spans: vec![span],
                    event_ids: vec![event.0],
                });
            }
        }
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
        matchers: &[crate::api::rule::ReturnedMemberCallMatcher],
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
                .returned_member_reads
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
                            .flat_map(|(_, occurrences)| occurrences.iter().copied())
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
                CallProvenance::Any => self.global_constructors.get(&constructor.name),
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
        let symbol = symbol.into();
        match kind {
            ApiMatchKind::Call => {
                self.calls.push(symbol, FactId(u32::MAX), span);
            }
            ApiMatchKind::MemberCall => {
                self.member_calls.push(symbol, FactId(u32::MAX), span);
            }
            ApiMatchKind::MemberRead => {
                self.member_reads.push(symbol, FactId(u32::MAX), span);
            }
            ApiMatchKind::Import => {
                self.imports.push(symbol, FactId(u32::MAX), span);
            }
            ApiMatchKind::StringLiteral => {
                self.string_literals.push(symbol, FactId(u32::MAX), span);
            }
            ApiMatchKind::Class => {
                self.classes.push(symbol, FactId(u32::MAX), span);
            }
            ApiMatchKind::Constructor => {
                self.constructors.push(symbol, FactId(u32::MAX), span);
            }
            ApiMatchKind::CallArgument => {}
        }
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
        use super::super::fact_builder::build_test_stream;
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
