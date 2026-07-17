//! Projection from the immutable fact stream into occurrence indexes.
//!
//! This is the only fact-to-index projection. It records every reusable
//! occurrence without consulting selected rules; query selection happens only
//! after normalization so catalog order cannot affect the shared model.

use super::{
    FactPayload, FactStream, MatcherFacts, SymbolCallProvenance, SymbolMemberProvenance,
    canonical_rooted_chain,
    occurrence::{InstanceMemberKey, ModuleExportKey},
};

impl MatcherFacts {
    /// Sort and deduplicate every occurrence index after fact collection.
    /// Queries rely on this normalization for deterministic output and binary
    /// search; keeping it as one operation prevents a newly added index from
    /// being accidentally left in insertion order.
    pub(in crate::analysis) fn normalize_occurrences(&mut self) {
        self.call_indexes.calls.normalize();
        self.call_indexes.global_calls.normalize();
        self.call_indexes.module_calls.normalize();
        self.members.calls.normalize();
        self.members.rooted_calls.normalize();
        self.members.module_calls.normalize();
        self.members.reads.normalize();
        self.members.rooted_reads.normalize();
        self.members.module_reads.normalize();
        self.members.returned_calls.normalize();
        self.members.returned_reads.normalize();
        self.members.instance_calls.normalize();
        self.literals.imports.normalize();
        self.literals.strings.normalize();
        self.constructions.classes.normalize();
        self.constructions.module_classes.normalize();
        self.constructions.constructors.normalize();
        self.constructions.global_constructors.normalize();
        self.constructions.module_constructors.normalize();
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
            FactPayload::Call { .. } => self.record_call_fact(fact),

            FactPayload::MemberRead { .. } => self.record_member_read_fact(fact),

            FactPayload::Construction { .. } => self.record_construction_fact(fact),

            FactPayload::Import { module } => {
                self.literals
                    .imports
                    .push(module.clone(), fact.id, fact.span);
            }

            FactPayload::Reference {
                static_string: Some(value),
                ..
            } => {
                self.literals
                    .strings
                    .push(value.clone(), fact.id, fact.span);
            }

            FactPayload::Class { name, provenance } => {
                if !name.is_empty() {
                    self.constructions
                        .classes
                        .push(name.clone(), fact.id, fact.span);
                }
                if let Some((module, export)) = provenance {
                    self.constructions.module_classes.push(
                        ModuleExportKey::new(module, export),
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

    fn record_call_fact(&mut self, fact: &super::super::facts::SemanticFact) {
        let FactPayload::Call {
            callee_name,
            callee_span,
            call_provenance,
            ..
        } = &fact.payload
        else {
            return;
        };
        if let Some(name) = callee_name {
            self.call_indexes
                .calls
                .push(name.clone(), fact.id, *callee_span);
        }
        match call_provenance {
            SymbolCallProvenance::Global { name } => {
                self.call_indexes
                    .global_calls
                    .push(name.clone(), fact.id, *callee_span);
            }
            SymbolCallProvenance::ModuleExport { module, export } => {
                self.call_indexes.module_calls.push(
                    ModuleExportKey::new(module, export),
                    fact.id,
                    *callee_span,
                );
                self.members.module_calls.push(
                    ModuleExportKey::new(module, export),
                    fact.id,
                    *callee_span,
                );
            }
            SymbolCallProvenance::Local
            | SymbolCallProvenance::Unknown(_)
            | SymbolCallProvenance::Ambiguous => {}
        }
        self.record_call_paths(fact);
        self.record_call_special_cases(fact);
    }

    fn record_call_paths(&mut self, fact: &super::super::facts::SemanticFact) {
        let FactPayload::Call {
            syntactic_chain,
            rooted_chain,
            module_member,
            returned_member,
            instance_class,
            callee_span,
            ..
        } = &fact.payload
        else {
            return;
        };
        let span = *callee_span;
        if let Some(chain) = syntactic_chain {
            self.members.calls.push(chain.clone(), fact.id, span);
        }
        if let Some(chain) = rooted_chain {
            self.members.rooted_calls.push(
                canonical_rooted_chain(chain).to_string(),
                fact.id,
                span,
            );
        }
        if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) = module_member {
            self.call_indexes.module_calls.push(
                ModuleExportKey::new(module, member),
                fact.id,
                span,
            );
            self.members
                .module_calls
                .push(ModuleExportKey::new(module, member), fact.id, span);
        }
        if let Some((source, member)) = returned_member {
            self.members
                .returned_calls
                .push(ModuleExportKey::new(source, member), fact.id, span);
        }
        if let Some((module, export)) = instance_class
            && let Some(member_name) = syntactic_chain
                .as_ref()
                .and_then(|chain| chain.rsplit('.').next())
        {
            self.members.instance_calls.push(
                InstanceMemberKey::new(module, export, member_name),
                fact.id,
                span,
            );
        }
    }

    fn record_call_special_cases(&mut self, fact: &super::super::facts::SemanticFact) {
        let FactPayload::Call {
            rooted_chain,
            unwrap,
            callee_span,
            ..
        } = &fact.payload
        else {
            return;
        };
        if rooted_chain
            .as_deref()
            .is_some_and(|chain| chain == "Function")
        {
            self.call_indexes
                .global_calls
                .push("Function".to_string(), fact.id, *callee_span);
            self.call_indexes
                .calls
                .push("Function".to_string(), fact.id, *callee_span);
        }
        if let Some(unwrap) = unwrap
            && !unwrap.chain.is_empty()
        {
            self.members
                .calls
                .push(unwrap.chain.clone(), fact.id, *callee_span);
            self.members.rooted_calls.push(
                canonical_rooted_chain(&unwrap.chain).to_string(),
                fact.id,
                *callee_span,
            );
        }
    }

    fn record_member_read_fact(&mut self, fact: &super::super::facts::SemanticFact) {
        let FactPayload::MemberRead {
            syntactic_chain,
            rooted_chain,
            module_member,
            returned_member,
            ..
        } = &fact.payload
        else {
            return;
        };
        if let Some(chain) = syntactic_chain {
            self.members.reads.push(chain.clone(), fact.id, fact.span);
        }
        if let Some(chain) = rooted_chain {
            self.members.rooted_reads.push(
                canonical_rooted_chain(chain).to_string(),
                fact.id,
                fact.span,
            );
        }
        if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) = module_member {
            self.members.module_reads.push(
                ModuleExportKey::new(module, member),
                fact.id,
                fact.span,
            );
            self.constructions
                .classes
                .push(member.clone(), fact.id, fact.span);
        }
        if let Some((source, member)) = returned_member {
            self.members.returned_reads.push(
                ModuleExportKey::new(source, member),
                fact.id,
                fact.span,
            );
        }
    }

    fn record_construction_fact(&mut self, fact: &super::super::facts::SemanticFact) {
        let FactPayload::Construction {
            callee_name,
            callee_span,
            provenance,
            ..
        } = &fact.payload
        else {
            return;
        };
        if let Some(name) = callee_name {
            self.constructions
                .constructors
                .push(name.clone(), fact.id, *callee_span);
        }
        match provenance {
            SymbolCallProvenance::Global { name } => {
                self.constructions
                    .global_constructors
                    .push(name.clone(), fact.id, *callee_span);
            }
            SymbolCallProvenance::ModuleExport { module, export } => {
                self.constructions.module_constructors.push(
                    ModuleExportKey::new(module, export),
                    fact.id,
                    *callee_span,
                );
            }
            SymbolCallProvenance::Local
            | SymbolCallProvenance::Unknown(_)
            | SymbolCallProvenance::Ambiguous => {}
        }
    }
}
