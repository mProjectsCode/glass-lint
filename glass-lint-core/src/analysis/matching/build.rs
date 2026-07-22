//! Projection from the immutable fact stream into occurrence indexes.
//!
//! This is the only fact-to-index projection. It records every reusable
//! occurrence without consulting selected rules; query selection happens only
//! after normalization so catalog order cannot affect the shared model.

use crate::analysis::{
    SymbolPath,
    facts::{ClassFactRole, SemanticFact},
    matching::{
        FactPayload, FactStream, OccurrenceIndexes, SymbolCallProvenance, SymbolMemberProvenance,
        occurrence::{InstanceMemberKey, ModuleExportKey, ReturnedMemberKey},
    },
    name::NameTable,
    value::NamePath,
};

impl OccurrenceIndexes {
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
        #[cfg(test)]
        {
            if let Some(names) = stream.names() {
                self.test_names = names.clone();
            }
        }
        // This is the sole projection from semantic facts into shared matcher
        // indexes. Rule selection must happen later, in query code.
        let values = stream.values();
        stream.facts().iter().for_each(|fact| {
            self.record_fact(
                fact,
                stream.names().expect("valid stream has names"),
                values,
            );
        });
    }

    fn record_fact(
        &mut self,
        fact: &SemanticFact,
        names: &NameTable,
        values: Option<&crate::analysis::value::ValueTable>,
    ) {
        match &fact.payload {
            FactPayload::Call { .. } => self.record_call_fact(fact, names),

            FactPayload::MemberRead { .. } => self.record_member_read_fact(fact, names),

            FactPayload::Construction { .. } => self.record_construction_fact(fact),

            FactPayload::Import { module } => {
                self.literals
                    .imports
                    .push(module.clone().into(), fact.id, fact.span);
            }

            FactPayload::Reference { value, .. } => {
                if let Some(static_string) =
                    values
                        .and_then(|v| v.get(*value))
                        .and_then(|val| match val {
                            crate::analysis::value::Value::StaticString(s) => Some(s),
                            _ => None,
                        })
                {
                    self.literals
                        .strings
                        .push(static_string.clone().into(), fact.id, fact.span);
                }
            }

            FactPayload::Class {
                name,
                provenance,
                role,
            } => {
                if matches!(role, ClassFactRole::Declaration)
                    && let Some(name) = name
                {
                    self.constructions
                        .classes
                        .push(name.clone(), fact.id, fact.span);
                }
                if let Some((module, export)) = provenance {
                    self.constructions.module_classes.push(
                        ModuleExportKey::new(module.clone(), export.clone()),
                        fact.id,
                        fact.span,
                    );
                }
            }

            // Declaration, Assignment, PropertyWrite, Function, Control
            // facts do not contribute to occurrence indexes.
            FactPayload::Declaration { .. }
            | FactPayload::Assignment { .. }
            | FactPayload::PropertyWrite { .. }
            | FactPayload::Function { .. }
            | FactPayload::Control { .. } => {}
        }
    }

    fn record_call_fact(&mut self, fact: &SemanticFact, _names: &NameTable) {
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
            self.call_indexes.calls.push(*name, fact.id, *callee_span);
        }
        match call_provenance {
            SymbolCallProvenance::Global { name } => {
                self.call_indexes
                    .global_calls
                    .push(name.clone(), fact.id, *callee_span);
            }
            SymbolCallProvenance::ModuleExport { module, export } => {
                self.call_indexes.module_calls.push(
                    ModuleExportKey::new(module.clone(), export.clone()),
                    fact.id,
                    *callee_span,
                );
                self.members.module_calls.push(
                    ModuleExportKey::new(module.clone(), export.clone()),
                    fact.id,
                    *callee_span,
                );
            }
            SymbolCallProvenance::Local | SymbolCallProvenance::Unknown(_) => {}
        }
        self.record_call_paths(fact);
        self.record_call_special_cases(fact);
    }

    fn record_call_paths(&mut self, fact: &SemanticFact) {
        let FactPayload::Call {
            syntactic_chain,
            syntactic_path,
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
        if let Some(chain) = syntactic_path {
            self.members.calls.push(chain.clone(), fact.id, span);
        }
        if let Some(chain) = rooted_chain {
            self.members.rooted_calls.push(chain.clone(), fact.id, span);
        }
        if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) = module_member {
            self.call_indexes.module_calls.push(
                ModuleExportKey::new(module.clone(), member.clone()),
                fact.id,
                span,
            );
            self.members.module_calls.push(
                ModuleExportKey::new(module.clone(), member.clone()),
                fact.id,
                span,
            );
        }
        if let Some((source, member)) = returned_member {
            self.members.returned_calls.push(
                ReturnedMemberKey::new(source.clone(), member.clone()),
                fact.id,
                span,
            );
        }
        if let Some((module, export)) = instance_class
            && let Some(member_name) = syntactic_chain.as_ref().and_then(SymbolPath::last_segment)
        {
            self.members.instance_calls.push(
                InstanceMemberKey::new(module.clone(), export.clone(), member_name),
                fact.id,
                span,
            );
        }
    }

    fn record_call_special_cases(&mut self, fact: &SemanticFact) {
        let FactPayload::Call {
            unwrap,
            callee_span,
            ..
        } = &fact.payload
        else {
            return;
        };
        if let Some(unwrap) = unwrap
            && let Some(chain) = &unwrap.chain_path
            && !unwrap.chain.is_empty()
        {
            self.members
                .calls
                .push(chain.clone(), fact.id, *callee_span);
            self.members
                .rooted_calls
                .push(chain.clone(), fact.id, *callee_span);
        }
    }

    fn record_member_read_fact(&mut self, fact: &SemanticFact, names: &NameTable) {
        let FactPayload::MemberRead {
            syntactic_chain,
            syntactic_path,
            rooted_chain,
            module_member,
            returned_member,
            ..
        } = &fact.payload
        else {
            return;
        };
        if let Some(chain) = syntactic_path {
            self.members.reads.push(chain.clone(), fact.id, fact.span);
        } else if let Some(chain) = syntactic_chain
            && let Some(chain) = NamePath::from_symbol_path(chain, names)
        {
            self.members.reads.push(chain, fact.id, fact.span);
        }
        if let Some(chain) = rooted_chain {
            self.members
                .rooted_reads
                .push(chain.clone(), fact.id, fact.span);
        }
        if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) = module_member {
            self.members.module_reads.push(
                ModuleExportKey::new(module.clone(), member.clone()),
                fact.id,
                fact.span,
            );
            self.constructions
                .classes
                .push(member.clone(), fact.id, fact.span);
        }
        if let Some((source, member)) = returned_member {
            self.members.returned_reads.push(
                ReturnedMemberKey::new(source.clone(), member.clone()),
                fact.id,
                fact.span,
            );
        }
    }

    fn record_construction_fact(&mut self, fact: &SemanticFact) {
        let FactPayload::Construction {
            callee_name,
            callee_span,
            provenance,
            ..
        } = &fact.payload
        else {
            return;
        };
        if let Some(name) = callee_name
            && matches!(provenance, SymbolCallProvenance::Global { .. })
        {
            self.constructions
                .constructors
                .push(*name, fact.id, *callee_span);
        }
        match provenance {
            SymbolCallProvenance::Global { name } => {
                self.constructions
                    .global_constructors
                    .push(name.clone(), fact.id, *callee_span);
            }
            SymbolCallProvenance::ModuleExport { module, export } => {
                self.constructions.module_constructors.push(
                    ModuleExportKey::new(module.clone(), export.clone()),
                    fact.id,
                    *callee_span,
                );
            }
            SymbolCallProvenance::Local | SymbolCallProvenance::Unknown(_) => {}
        }
    }
}
