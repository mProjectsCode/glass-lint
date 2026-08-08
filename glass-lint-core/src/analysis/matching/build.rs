//! Projection from the immutable fact stream into occurrence indexes.
//!
//! This is the only fact-to-index projection. It records every reusable
//! occurrence without consulting selected rules; query selection happens only
//! after normalization so catalog order cannot affect the shared model.

use glass_lint_datastructures::{NamePath, NameTable};
use smol_str::SmolStr;

use crate::analysis::{
    facts::{CallUnwrap, ClassFactRole, FactId, FactPayload, FactStream, Frozen, SemanticFact},
    matching::{
        OccurrenceIndexes,
        occurrence::{InstanceMemberKey, ModuleExportKey, Occurrence, ReturnedMemberKey},
    },
    syntax::{SymbolCallProvenance, SymbolMemberProvenance},
};

struct CallProjection<'a> {
    id: FactId,
    callee_span: glass_lint_datastructures::ByteRange,
    callee_name: Option<glass_lint_datastructures::NameId>,
    call_provenance: &'a SymbolCallProvenance,
    syntactic_path: Option<&'a NamePath>,
    rooted_chain: Option<&'a NamePath>,
    module_member: Option<&'a SymbolMemberProvenance>,
    returned_member: Option<&'a (NamePath, NamePath)>,
    instance_class: Option<&'a (SmolStr, SmolStr)>,
    unwrap: Option<&'a CallUnwrap>,
}

impl<'a> CallProjection<'a> {
    fn from_fact(fact: &'a SemanticFact) -> Option<Self> {
        let FactPayload::Call {
            callee_name,
            callee_span,
            call_provenance,
            syntactic_path,
            rooted_chain,
            module_member,
            returned_member,
            instance_class,
            unwrap,
            ..
        } = &fact.payload
        else {
            return None;
        };
        Some(Self {
            id: fact.id,
            callee_span: *callee_span,
            callee_name: *callee_name,
            call_provenance,
            syntactic_path: syntactic_path.as_ref(),
            rooted_chain: rooted_chain.as_ref(),
            module_member: module_member.as_ref(),
            returned_member: returned_member.as_ref(),
            instance_class: instance_class.as_ref(),
            unwrap: unwrap.as_deref(),
        })
    }

    fn occurrence(&self) -> Occurrence {
        Occurrence::new(self.id, self.callee_span)
    }
}

impl OccurrenceIndexes {
    fn record_module_call(&mut self, key: ModuleExportKey, occurrence: Occurrence) {
        self.call_indexes
            .record_module_call(key.clone(), occurrence);
        self.members.record_module_call(key, occurrence);
    }

    /// Deduplicate every occurrence index after fact collection.
    /// Entries are already in monotonically increasing `(event, span)` order
    /// because `build_from_stream` iterates facts in FactId order.
    /// Queries rely on this normalization for deterministic output.
    pub(in crate::analysis) fn normalize_occurrences(&mut self) {
        self.call_indexes.normalize();
        self.members.normalize();
        self.constructions.normalize();
        self.literals.normalize();
    }

    pub(in crate::analysis) fn build_from_stream(&mut self, stream: &FactStream<Frozen>) {
        #[cfg(test)]
        {
            self.test_names = stream.names().clone();
        }
        let values = stream.values();
        for fact in stream.facts() {
            self.record_fact(fact, stream.names(), values);
        }
    }

    pub(in crate::analysis) fn record_fact(
        &mut self,
        fact: &SemanticFact,
        names: &NameTable,
        values: &crate::analysis::model::value::ValueTable,
    ) {
        // This is the sole projection from semantic facts into shared matcher
        // indexes. Rule selection must happen later, in query code.
        match &fact.payload {
            FactPayload::Call { .. } => self.record_call_fact(fact, names),

            FactPayload::MemberRead { .. } => self.record_member_read_fact(fact, names),

            FactPayload::PropertyWrite {
                rooted_chain: Some(chain),
                ..
            } => {
                self.members
                    .record_rooted_write(chain.clone(), Occurrence::new(fact.id, fact.span));
            }

            FactPayload::Construction { .. } => self.record_construction_fact(fact),

            FactPayload::Import { module } => {
                self.literals
                    .record_import(module.clone(), Occurrence::new(fact.id, fact.span));
            }

            FactPayload::Reference {
                value,
                static_string_origin,
                ..
            } => {
                if let Some(static_string) = values.get(*value).and_then(|val| match val {
                    crate::analysis::model::value::Value::StaticString(s) => Some(s),
                    _ => None,
                }) {
                    self.literals.record_string(
                        static_string.clone(),
                        Occurrence::new(fact.id, static_string_origin.unwrap_or(fact.span)),
                    );
                }
            }

            FactPayload::Class {
                name,
                provenance,
                role,
            } => {
                if let Some(name) = name {
                    self.constructions
                        .record_class(name.clone(), Occurrence::new(fact.id, fact.span));
                }
                if !matches!(role, ClassFactRole::Declaration)
                    && let Some((module, export)) = provenance
                {
                    self.constructions.record_module_class(
                        ModuleExportKey::new(module.clone(), export.clone()),
                        Occurrence::new(fact.id, fact.span),
                    );
                }
            }

            // Declaration, Assignment, PropertyWrite, Function, Control
            // facts do not contribute to occurrence indexes.
            FactPayload::Declaration { .. }
            | FactPayload::Assignment { .. }
            | FactPayload::PropertyWrite { .. }
            | FactPayload::Function { .. }
            | FactPayload::Control { .. }
            | FactPayload::Return { .. } => {}
        }
    }

    fn record_call_fact(&mut self, fact: &SemanticFact, names: &NameTable) {
        let Some(call) = CallProjection::from_fact(fact) else {
            return;
        };
        if let Some(name) = call.callee_name {
            self.call_indexes.record_call(name, call.occurrence());
        }
        match call.call_provenance {
            SymbolCallProvenance::Global { name } => {
                self.call_indexes
                    .record_global_call(name.clone(), call.occurrence());
            }
            SymbolCallProvenance::ModuleExport { module, export } => {
                self.record_module_call(
                    ModuleExportKey::new(module.clone(), export.clone()),
                    call.occurrence(),
                );
            }
            SymbolCallProvenance::Local | SymbolCallProvenance::Unknown(_) => {}
        }
        self.record_call_paths(&call, names);
        self.record_call_special_cases(&call);
    }

    fn record_call_paths(&mut self, call: &CallProjection<'_>, names: &NameTable) {
        if let Some(chain) = call.syntactic_path {
            self.members.record_call(chain.clone(), call.occurrence());
        }
        if let Some(chain) = call.rooted_chain {
            self.members
                .record_rooted_call(chain.clone(), call.occurrence());
        }
        if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) = call.module_member
        {
            self.record_module_call(
                ModuleExportKey::new(module.clone(), member.clone()),
                call.occurrence(),
            );
        }
        if let Some((source, member)) = call.returned_member {
            self.members.record_returned_call(
                ReturnedMemberKey::new(source.clone(), member.clone()),
                call.occurrence(),
            );
        }
        if let Some((module, export)) = call.instance_class
            && let Some(member_name) = call
                .syntactic_path
                .and_then(NamePath::last_segment)
                .copied()
                .and_then(|id| names.resolve(id))
        {
            self.members.record_instance_call(
                InstanceMemberKey::new(module.clone(), export.clone(), SmolStr::new(member_name)),
                call.occurrence(),
            );
        }
    }

    fn record_call_special_cases(&mut self, call: &CallProjection<'_>) {
        if let Some(unwrap) = call.unwrap
            && let Some(chain) = &unwrap.chain_path
            && chain.first_segment().is_some()
        {
            let occurrence = call.occurrence();
            self.members.record_call(chain.clone(), occurrence);
            self.members.record_rooted_call(chain.clone(), occurrence);
        }
    }

    fn record_member_read_fact(&mut self, fact: &SemanticFact, _names: &NameTable) {
        let FactPayload::MemberRead {
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
            self.members
                .record_read(chain.clone(), Occurrence::new(fact.id, fact.span));
        }
        if let Some(chain) = rooted_chain {
            self.members
                .record_rooted_read(chain.clone(), Occurrence::new(fact.id, fact.span));
        }
        if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) = module_member {
            self.members.record_module_read(
                ModuleExportKey::new(module.clone(), member.clone()),
                Occurrence::new(fact.id, fact.span),
            );
        }
        if let Some((source, member)) = returned_member {
            self.members.record_returned_read(
                ReturnedMemberKey::new(source.clone(), member.clone()),
                Occurrence::new(fact.id, fact.span),
            );
        }
    }

    fn record_construction_fact(&mut self, fact: &SemanticFact) {
        let FactPayload::Construction {
            callee_name,
            callee_span,
            provenance,
            rooted_chain,
            ..
        } = &fact.payload
        else {
            return;
        };
        if let Some(name) = callee_name {
            self.constructions
                .record_constructor(*name, Occurrence::new(fact.id, *callee_span));
        }
        if let Some(chain) = rooted_chain {
            self.constructions
                .record_rooted_constructor(chain.clone(), Occurrence::new(fact.id, *callee_span));
        }
        match provenance {
            SymbolCallProvenance::Global { name } => {
                self.constructions.record_global_constructor(
                    name.clone(),
                    Occurrence::new(fact.id, *callee_span),
                );
            }
            SymbolCallProvenance::ModuleExport { module, export } => {
                self.constructions.record_module_constructor(
                    ModuleExportKey::new(module.clone(), export.clone()),
                    Occurrence::new(fact.id, *callee_span),
                );
            }
            SymbolCallProvenance::Local | SymbolCallProvenance::Unknown(_) => {}
        }
    }
}
