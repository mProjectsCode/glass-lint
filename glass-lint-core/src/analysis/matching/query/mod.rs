use glass_lint_datastructures::{NameTable, SymbolPath};

use crate::{
    analysis::matching::{
        ClassificationEvidence, LinkedOccurrenceView, OccurrenceIndexes, OccurrenceSelection,
        occurrence::ReturnedMemberKey, push_owned_evidence,
    },
    api::compiler::{
        physical::PhysicalRoot,
        rule::{CompiledMatcherPlan, EventSpec, EvidenceDescriptor, IdentityConstraint},
    },
};

mod view;
use view::EventIndexView;

pub(in crate::analysis) enum IndexedRoot<'a> {
    IndexedScan {
        identity: &'a IdentityConstraint,
        event: &'a EventSpec,
        evidence: &'a EvidenceDescriptor,
    },
    ReturnedSubject {
        producer: &'a IdentityConstraint,
        member: &'a SymbolPath,
        event: &'a EventSpec,
        evidence: &'a EvidenceDescriptor,
    },
    InstanceSubject {
        constructor: &'a IdentityConstraint,
        member: &'a SymbolPath,
        evidence: &'a EvidenceDescriptor,
    },
}

impl<'a> IndexedRoot<'a> {
    fn from_physical(root: &'a PhysicalRoot) -> Option<Self> {
        match root {
            PhysicalRoot::IndexedScan {
                identity,
                event,
                evidence,
            } => Some(Self::IndexedScan {
                identity,
                event,
                evidence,
            }),
            PhysicalRoot::ReturnedSubject {
                producer,
                member,
                event,
                evidence,
            } => Some(Self::ReturnedSubject {
                producer,
                member,
                event,
                evidence,
            }),
            PhysicalRoot::InstanceSubject {
                constructor,
                member,
                evidence,
                ..
            } => Some(Self::InstanceSubject {
                constructor,
                member,
                evidence,
            }),
            PhysicalRoot::ConstrainedScan { .. } | PhysicalRoot::Lifecycle { .. } => None,
        }
    }
}

pub(in crate::analysis) struct IndexedRootIter<'a> {
    roots: std::slice::Iter<'a, PhysicalRoot>,
}

impl<'a> IndexedRootIter<'a> {
    pub(in crate::analysis) fn from_plan(plan: &'a CompiledMatcherPlan) -> Self {
        Self {
            roots: plan.physical_roots().iter(),
        }
    }
}

impl<'a> Iterator for IndexedRootIter<'a> {
    type Item = IndexedRoot<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.roots.find_map(IndexedRoot::from_physical)
    }
}

impl OccurrenceIndexes {
    pub(in crate::analysis) fn evidence_for_indexed_with_overlay<'a>(
        &'a self,
        roots: IndexedRootIter<'a>,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
        names: &NameTable,
    ) -> Vec<ClassificationEvidence> {
        // Return raw groups for the shared evidence normalization boundary;
        // project evidence is merged here and normalized exactly once by the
        // report-facing projection model.
        let mut evidence = Vec::new();
        for root in roots {
            match root {
                IndexedRoot::IndexedScan {
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
                IndexedRoot::ReturnedSubject {
                    producer,
                    member,
                    event,
                    evidence: ev,
                    ..
                } => {
                    if let Some(occurrences) =
                        self.occurrences_for_returned(producer, member, event, names)
                    {
                        push_owned_evidence(&mut evidence, ev.kind, ev.symbol.clone(), occurrences);
                    }
                }
                IndexedRoot::InstanceSubject {
                    constructor,
                    member,
                    evidence: ev,
                    ..
                } => {
                    if let Some(occurrences) = self.occurrences_for_instance(constructor, member) {
                        push_owned_evidence(&mut evidence, ev.kind, ev.symbol.clone(), occurrences);
                    }
                }
            }
        }
        evidence
    }

    /// Resolve an indexed scan (direct event lookup).
    pub(in crate::analysis) fn occurrences_for_indexed<'a>(
        &'a self,
        identity: &'a IdentityConstraint,
        event: &'a EventSpec,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
        names: &NameTable,
    ) -> Option<OccurrenceSelection<'a>> {
        let view = self.build_event_view(event);
        view.resolve(identity, names, overlay)
    }

    /// Resolve a returned-subject scan.
    fn occurrences_for_returned<'a>(
        &'a self,
        identity: &'a IdentityConstraint,
        member: &SymbolPath,
        event: &EventSpec,
        names: &'a NameTable,
    ) -> Option<OccurrenceSelection<'a>> {
        let rooted_path = match identity {
            IdentityConstraint::Rooted { path } => names.lookup_path(path)?,
            _ => return None,
        };
        let member_path = names.lookup_path(member)?;
        let predicate = |key: &ReturnedMemberKey| {
            (self
                .environment
                .global_object_name_paths_match(&rooted_path, key.source(), names)
                || key.source().is_equal_or_descendant_of(&rooted_path))
                && member_path == *key.member()
        };
        match event {
            EventSpec::MemberCall { .. } => self.members.returned_calls().matching(predicate),
            EventSpec::MemberRead { .. } => self.members.returned_reads().matching(predicate),
            _ => None,
        }
    }

    /// Resolve an instance-subject scan.
    fn occurrences_for_instance<'a>(
        &'a self,
        constructor: &IdentityConstraint,
        member: &SymbolPath,
    ) -> Option<OccurrenceSelection<'a>> {
        self.members
            .instance_calls()
            .matching(|key| match constructor {
                IdentityConstraint::ModuleExport {
                    module: expected_module,
                    export: expected_export,
                } => {
                    key.module() == expected_module
                        && key.export() == expected_export
                        && member.eq_chain(key.member())
                }
                IdentityConstraint::PackageModuleExport { module, export } => {
                    module.matches(key.module())
                        && key.export() == export
                        && member.eq_chain(key.member())
                }
                _ => false,
            })
    }

    fn build_event_view<'a>(&'a self, event: &'a EventSpec) -> EventIndexView<'a> {
        let env = &self.environment;
        match event {
            EventSpec::Call => EventIndexView::Call {
                names: self.call_indexes.calls(),
                module: self.call_indexes.module_calls(),
                global: self.call_indexes.global_calls(),
            },
            EventSpec::MemberCall { member } => EventIndexView::MemberCall {
                member,
                paths: self.members.calls(),
                module: self.members.module_calls(),
                rooted: self.members.rooted_calls(),
                environment: env,
            },
            EventSpec::MemberRead { member } => EventIndexView::MemberRead {
                member,
                paths: self.members.reads(),
                module: self.members.module_reads(),
                rooted: self.members.rooted_reads(),
                environment: env,
            },
            EventSpec::PropertyWrite { property } => EventIndexView::PropertyWrite {
                property,
                writes: self.members.rooted_writes(),
                environment: env,
            },
            EventSpec::ClassReference => EventIndexView::ClassReference {
                strings: self.constructions.classes(),
                module: self.constructions.module_classes(),
            },
            EventSpec::Construct => EventIndexView::Construct {
                names: self.constructions.constructors(),
                module: self.constructions.module_constructors(),
                global: self.constructions.global_constructors(),
                rooted: self.constructions.rooted_constructors(),
                environment: env,
            },
            EventSpec::Import => EventIndexView::Import {
                literals: self.literals.imports(),
            },
            EventSpec::StringReference => EventIndexView::StringReference {
                literals: self.literals.strings(),
            },
        }
    }
}
