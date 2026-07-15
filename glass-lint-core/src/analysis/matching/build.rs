//! Projection from the immutable fact stream into occurrence indexes.

use super::{
    FactPayload, FactStream, MatcherFacts, SymbolCallProvenance, SymbolMemberProvenance,
    canonical_rooted_chain,
};

struct CallFact<'a> {
    event: super::super::facts::FactId,
    span: swc_common::Span,
    callee_name: Option<&'a String>,
    call_provenance: &'a SymbolCallProvenance,
    syntactic_chain: Option<&'a String>,
    rooted_chain: Option<&'a String>,
    module_member: Option<&'a SymbolMemberProvenance>,
    returned_member: Option<&'a (String, String)>,
    instance_class: Option<&'a (String, String)>,
    unwrap: Option<&'a super::super::facts::CallUnwrap>,
}

struct MemberReadFact<'a> {
    event: super::super::facts::FactId,
    span: swc_common::Span,
    syntactic_chain: Option<&'a String>,
    rooted_chain: Option<&'a String>,
    module_member: Option<&'a SymbolMemberProvenance>,
    returned_member: Option<&'a (String, String)>,
}

struct ConstructionFact<'a> {
    event: super::super::facts::FactId,
    span: swc_common::Span,
    callee_name: Option<&'a String>,
    provenance: &'a SymbolCallProvenance,
}

impl MatcherFacts {
    /// Sort and deduplicate every occurrence index after fact collection.
    /// Queries rely on this normalization for deterministic output and binary
    /// search; keeping it as one operation prevents a newly added index from
    /// being accidentally left in insertion order.
    pub(in crate::analysis) fn normalize_occurrences(&mut self) {
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

    pub(in crate::analysis) fn build_from_stream(&mut self, stream: &FactStream) {
        // This is the sole projection from semantic facts into shared matcher
        // indexes. Rule selection must happen later, in query code.
        stream
            .facts()
            .iter()
            .for_each(|fact| self.record_fact(fact));
    }

    fn record_fact(&mut self, fact: &super::super::facts::SemanticFact) {
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
            } => self.record_call_fact(&CallFact {
                event: fact.id,
                span: *callee_span,
                callee_name: callee_name.as_ref(),
                call_provenance,
                syntactic_chain: syntactic_chain.as_ref(),
                rooted_chain: rooted_chain.as_ref(),
                module_member: module_member.as_ref(),
                returned_member: returned_member.as_ref(),
                instance_class: instance_class.as_ref(),
                unwrap: unwrap.as_deref(),
            }),

            FactPayload::MemberRead {
                syntactic_chain,
                rooted_chain,
                module_member,
                returned_member,
                ..
            } => self.record_member_read_fact(&MemberReadFact {
                event: fact.id,
                span: fact.span,
                syntactic_chain: syntactic_chain.as_ref(),
                rooted_chain: rooted_chain.as_ref(),
                module_member: module_member.as_ref(),
                returned_member: returned_member.as_ref(),
            }),

            FactPayload::Construction {
                callee_name,
                callee_span,
                provenance,
                ..
            } => self.record_construction_fact(&ConstructionFact {
                event: fact.id,
                span: *callee_span,
                callee_name: callee_name.as_ref(),
                provenance,
            }),

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
                    self.module_classes
                        .push((module.clone(), export.clone()), fact.id, fact.span);
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

    fn record_call_fact(&mut self, input: &CallFact<'_>) {
        if let Some(name) = input.callee_name {
            self.calls.push(name.clone(), input.event, input.span);
        }
        match input.call_provenance {
            SymbolCallProvenance::Global { name } => {
                self.global_calls
                    .push(name.clone(), input.event, input.span);
            }
            SymbolCallProvenance::ModuleExport { module, export } => {
                self.module_calls
                    .push((module.clone(), export.clone()), input.event, input.span);
                self.module_member_calls.push(
                    (module.clone(), export.clone()),
                    input.event,
                    input.span,
                );
            }
            SymbolCallProvenance::Local => {}
        }
        if let Some(chain) = input.syntactic_chain {
            self.member_calls
                .push(chain.clone(), input.event, input.span);
        }
        if let Some(chain) = input.rooted_chain {
            self.rooted_member_calls.push(
                canonical_rooted_chain(chain).to_string(),
                input.event,
                input.span,
            );
        }
        if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) =
            input.module_member
        {
            self.module_calls
                .push((module.clone(), member.clone()), input.event, input.span);
            self.module_member_calls.push(
                (module.clone(), member.clone()),
                input.event,
                input.span,
            );
        }
        if let Some((source, member)) = input.returned_member {
            self.returned_member_calls.push(
                (source.clone(), member.clone()),
                input.event,
                input.span,
            );
        }
        if let Some((module, export)) = input.instance_class
            && let Some(member_name) = input
                .syntactic_chain
                .as_ref()
                .and_then(|chain| chain.rsplit('.').next())
        {
            self.instance_member_calls.push(
                (module.clone(), export.clone(), member_name.to_string()),
                input.event,
                input.span,
            );
        }
        if input.rooted_chain.is_some_and(|chain| chain == "Function") {
            self.global_calls
                .push("Function".to_string(), input.event, input.span);
            self.calls
                .push("Function".to_string(), input.event, input.span);
        }
        if let Some(unwrap) = input.unwrap
            && !unwrap.chain.is_empty()
        {
            self.member_calls
                .push(unwrap.chain.clone(), input.event, input.span);
            self.rooted_member_calls.push(
                canonical_rooted_chain(&unwrap.chain).to_string(),
                input.event,
                input.span,
            );
        }
    }

    fn record_member_read_fact(&mut self, input: &MemberReadFact<'_>) {
        if let Some(chain) = input.syntactic_chain {
            self.member_reads
                .push(chain.clone(), input.event, input.span);
        }
        if let Some(chain) = input.rooted_chain {
            self.rooted_member_reads.push(
                canonical_rooted_chain(chain).to_string(),
                input.event,
                input.span,
            );
        }
        if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) =
            input.module_member
        {
            self.module_member_reads.push(
                (module.clone(), member.clone()),
                input.event,
                input.span,
            );
            self.classes.push(member.clone(), input.event, input.span);
        }
        if let Some((source, member)) = input.returned_member {
            self.returned_member_reads.push(
                (source.clone(), member.clone()),
                input.event,
                input.span,
            );
        }
    }

    fn record_construction_fact(&mut self, input: &ConstructionFact<'_>) {
        if let Some(name) = input.callee_name {
            self.constructors
                .push(name.clone(), input.event, input.span);
        }
        match input.provenance {
            SymbolCallProvenance::Global { name } => {
                self.global_constructors
                    .push(name.clone(), input.event, input.span);
            }
            SymbolCallProvenance::ModuleExport { module, export } => {
                self.module_constructors.push(
                    (module.clone(), export.clone()),
                    input.event,
                    input.span,
                );
            }
            SymbolCallProvenance::Local => {}
        }
    }
}
