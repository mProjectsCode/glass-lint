#[cfg(test)]
use glass_lint_datastructures::ByteRange;
use glass_lint_datastructures::{NameTable, SymbolPath};
#[cfg(test)]
use smol_str::SmolStr;

#[cfg(test)]
use crate::{analysis::facts::FactId, api::classification::MatchKind};
use crate::{
    analysis::matching::{
        CandidateOccurrences, ClassificationEvidence, LinkedOccurrenceView, OccurrenceIndexes,
        occurrence::ReturnedMemberKey, push_owned_evidence,
    },
    api::compiler::{
        physical::PhysicalRoot,
        rule::{CompiledMatcherPlan, EventPredicate, IdentityConstraint},
    },
};

mod view;
use view::EventIndexView;
pub(super) use view::private_network_match;

#[cfg(test)]
use crate::analysis::matching::occurrence::Occurrence;

impl OccurrenceIndexes {
    #[cfg(test)]
    pub(in crate::analysis) fn evidence_for(
        &self,
        plan: &CompiledMatcherPlan,
    ) -> Vec<ClassificationEvidence> {
        self.evidence_for_with_overlay(plan, None, &self.test_names)
    }

    pub(in crate::analysis) fn evidence_for_with_overlay<'a>(
        &'a self,
        plan: &CompiledMatcherPlan,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
        names: &NameTable,
    ) -> Vec<ClassificationEvidence> {
        // Return raw groups for the shared evidence normalization boundary;
        // project evidence is merged here and normalized exactly once by the
        // report-facing projection model.
        let mut evidence = Vec::new();
        for root in plan.physical_roots() {
            match root {
                PhysicalRoot::IndexedScan {
                    identity,
                    event,
                    evidence: ev,
                } => {
                    if let Some(occurrences) =
                        self.occurrences_for_indexed(identity, event, overlay, names)
                    {
                        push_owned_evidence(&mut evidence, ev.kind, ev.symbol.clone(), occurrences);
                    }
                }
                PhysicalRoot::ReturnedSubject {
                    producer,
                    member,
                    event,
                    evidence: ev,
                    ..
                } => {
                    if let Some(occurrences) =
                        self.occurrences_for_returned(producer, member, event, overlay, names)
                    {
                        push_owned_evidence(&mut evidence, ev.kind, ev.symbol.clone(), occurrences);
                    }
                }
                PhysicalRoot::InstanceSubject {
                    constructor,
                    member,
                    evidence: ev,
                    ..
                } => {
                    if let Some(occurrences) =
                        self.occurrences_for_instance(constructor, member, names)
                    {
                        push_owned_evidence(&mut evidence, ev.kind, ev.symbol.clone(), occurrences);
                    }
                }
                // Constrained scans are handled by the fact-stream projection path.
                // Lifecycle roots are handled by the flow projection path.
                PhysicalRoot::ConstrainedScan { .. } | PhysicalRoot::Lifecycle { .. } => {}
            }
        }
        evidence
    }

    /// Resolve an indexed scan (direct event lookup).
    pub(in crate::analysis) fn occurrences_for_indexed<'a>(
        &'a self,
        identity: &'a IdentityConstraint,
        event: &'a EventPredicate,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
        names: &NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
        let view = self.build_event_view(event);
        view.resolve(identity, names, overlay)
    }

    /// Resolve a returned-subject scan.
    fn occurrences_for_returned<'a>(
        &'a self,
        identity: &'a IdentityConstraint,
        member: &SymbolPath,
        event: &EventPredicate,
        _overlay: Option<&'a LinkedOccurrenceView<'a>>,
        names: &'a NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
        let predicate = |key: &ReturnedMemberKey| {
            names.resolve_path(key.source()).is_some_and(|source| {
                identity.root_or_descendant_matches(&source, &self.environment)
            }) && names
                .lookup_path(member)
                .is_some_and(|m| m == *key.member())
        };
        match event {
            EventPredicate::MemberCall { .. } => self.members.returned_calls().matching(predicate),
            EventPredicate::MemberRead { .. } => self.members.returned_reads().matching(predicate),
            _ => None,
        }
    }

    /// Resolve an instance-subject scan.
    fn occurrences_for_instance<'a>(
        &'a self,
        constructor: &IdentityConstraint,
        member: &SymbolPath,
        _names: &NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
        self.members
            .instance_calls()
            .matching(|key| match constructor {
                IdentityConstraint::ModuleExport {
                    module: expected_module,
                    export: expected_export,
                } => {
                    key.identity().module() == expected_module
                        && key.identity().export() == expected_export
                        && member.eq_chain(key.member())
                }
                IdentityConstraint::PackageModuleExport { module, export } => {
                    module.matches(key.identity().module())
                        && key.identity().export() == export
                        && member.eq_chain(key.member())
                }
                _ => false,
            })
    }

    // occurrences_for_clause, occurrences_for_subject, and
    // occurrences_for_event were removed in Phase 7.
    // The constrained evidence path now uses occurrences_for_indexed
    // directly, and returned/instance subject lookups use
    // occurrences_for_returned / occurrences_for_instance.

    fn build_event_view<'a>(&'a self, event: &'a EventPredicate) -> EventIndexView<'a> {
        let env = &self.environment;
        match event {
            EventPredicate::Call => EventIndexView::Call {
                names: self.call_indexes.calls(),
                module: self.call_indexes.module_calls(),
                global: self.call_indexes.global_calls(),
            },
            EventPredicate::MemberCall { member } => EventIndexView::MemberCall {
                member,
                paths: self.members.calls(),
                module: self.members.module_calls(),
                rooted: self.members.rooted_calls(),
                environment: env,
            },
            EventPredicate::MemberRead { member } => EventIndexView::MemberRead {
                member,
                paths: self.members.reads(),
                module: self.members.module_reads(),
                rooted: self.members.rooted_reads(),
                environment: env,
            },
            EventPredicate::PropertyWrite { property } => EventIndexView::PropertyWrite {
                property,
                writes: self.members.rooted_writes(),
                environment: env,
            },
            EventPredicate::ClassReference => EventIndexView::ClassReference {
                strings: self.constructions.classes(),
                module: self.constructions.module_classes(),
            },
            EventPredicate::Construct => EventIndexView::Construct {
                names: self.constructions.constructors(),
                module: self.constructions.module_constructors(),
                global: self.constructions.global_constructors(),
                rooted: self.constructions.rooted_constructors(),
                environment: env,
            },
            EventPredicate::Import => EventIndexView::Import {
                literals: self.literals.imports(),
            },
            EventPredicate::StringReference => EventIndexView::StringReference {
                literals: self.literals.strings(),
            },
        }
    }

    #[cfg(test)]
    pub(super) fn record(&mut self, kind: MatchKind, symbol: impl Into<SmolStr>, span: ByteRange) {
        let symbol = symbol.into();
        match kind {
            MatchKind::Call => {
                let name = self.test_name(symbol.as_str());
                self.call_indexes
                    .record_call(name, Occurrence::new(FactId::from_test(u32::MAX), span));
            }
            MatchKind::MemberCall => {
                let key = symbol
                    .split('.')
                    .map(|segment| self.test_name(segment))
                    .collect::<Vec<_>>();
                self.members.record_call(
                    glass_lint_datastructures::NamePath::from_ids(key),
                    Occurrence::new(FactId::from_test(u32::MAX), span),
                );
            }
            MatchKind::MemberRead => {
                let key = symbol
                    .split('.')
                    .map(|segment| self.test_name(segment))
                    .collect::<Vec<_>>();
                self.members.record_read(
                    glass_lint_datastructures::NamePath::from_ids(key),
                    Occurrence::new(FactId::from_test(u32::MAX), span),
                );
            }
            MatchKind::PropertyWrite => {
                let key = symbol
                    .split('.')
                    .map(|segment| self.test_name(segment))
                    .collect::<Vec<_>>();
                self.members.record_rooted_write(
                    glass_lint_datastructures::NamePath::from_ids(key),
                    Occurrence::new(FactId::from_test(u32::MAX), span),
                );
            }
            MatchKind::Import => {
                self.literals
                    .record_import(symbol, Occurrence::new(FactId::from_test(u32::MAX), span));
            }
            MatchKind::StringContains => {
                self.literals
                    .record_string(symbol, Occurrence::new(FactId::from_test(u32::MAX), span));
            }
            MatchKind::Class => {
                self.constructions
                    .record_class(symbol, Occurrence::new(FactId::from_test(u32::MAX), span));
            }
            MatchKind::Constructor => {
                let name = self.test_name(symbol.as_str());
                self.constructions
                    .record_constructor(name, Occurrence::new(FactId::from_test(u32::MAX), span));
            }
            MatchKind::CallArgument => {}
        }
    }
}
