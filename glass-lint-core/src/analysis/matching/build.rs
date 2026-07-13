//! Projection from the immutable fact stream into occurrence indexes.

use super::{
    FactPayload, FactStream, MatcherFacts, SymbolCallProvenance, SymbolMemberProvenance,
    canonical_rooted_chain,
};

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

    #[allow(clippy::too_many_lines)]
    pub(in crate::analysis) fn build_from_stream(&mut self, stream: &FactStream) {
        // This is the sole projection from semantic facts into shared matcher
        // indexes. Rule selection must happen later, in query code.
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
}
