#[cfg(test)]
use glass_lint_datastructures::ByteRange;
use glass_lint_datastructures::NameTable;
#[cfg(test)]
use smol_str::SmolStr;

#[cfg(test)]
use crate::{analysis::facts::FactId, api::classification::MatchKind};
use crate::{
    analysis::matching::{
        CandidateOccurrences, ClassificationEvidence, LinkedOccurrenceView, OccurrenceIndexes,
        occurrence::ReturnedMemberKey, push_owned_evidence,
    },
    api::compiler::rule::{
        EventPredicate, IdentityConstraint, QueryClause, QueryPlan, SubjectConstraint,
    },
};

mod view;
use view::EventIndexView;

impl OccurrenceIndexes {
    #[cfg(test)]
    pub(in crate::analysis) fn evidence_for(
        &self,
        plan: &QueryPlan,
    ) -> Vec<ClassificationEvidence> {
        self.evidence_for_with_overlay(plan, None, &self.test_names)
    }

    pub(in crate::analysis) fn evidence_for_with_overlay<'a>(
        &'a self,
        plan: &QueryPlan,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
        names: &NameTable,
    ) -> Vec<ClassificationEvidence> {
        let mut evidence = Vec::new();
        for clause in plan.clauses() {
            if !clause.constraints.is_empty() {
                continue;
            }
            if let Some(occurrences) = self.occurrences_for_clause(clause, overlay, names) {
                push_owned_evidence(
                    &mut evidence,
                    clause.evidence.kind,
                    clause.evidence.symbol.clone(),
                    occurrences,
                );
            }
        }
        evidence.sort_by(|left, right| {
            let left_first = left.occurrences.first().map(|occurrence| occurrence.span);
            let right_first = right.occurrences.first().map(|occurrence| occurrence.span);
            left_first
                .cmp(&right_first)
                .then_with(|| left.kind.cmp(&right.kind))
                .then_with(|| left.symbol.as_str().cmp(right.symbol.as_str()))
        });
        evidence
    }

    pub(in crate::analysis) fn occurrences_for_clause<'a>(
        &'a self,
        clause: &'a QueryClause,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
        names: &NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
        if !matches!(clause.subject, SubjectConstraint::Direct) {
            return self.occurrences_for_subject(clause, overlay, names);
        }
        self.occurrences_for_event(clause, overlay, names)
    }

    fn occurrences_for_subject<'a>(
        &'a self,
        clause: &'a QueryClause,
        _overlay: Option<&'a LinkedOccurrenceView<'a>>,
        names: &NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
        match (&clause.event, &clause.subject) {
            (
                EventPredicate::MemberCall { member } | EventPredicate::MemberRead { member },
                SubjectConstraint::ReturnedFrom { .. },
            ) => {
                let predicate = |key: &ReturnedMemberKey| {
                    names.resolve_path(key.source()).is_some_and(|source| {
                        clause
                            .identity
                            .root_or_descendant_matches(&source, &self.environment)
                    }) && names
                        .lookup_path(member)
                        .is_some_and(|m| m == *key.member())
                };
                match &clause.event {
                    EventPredicate::MemberCall { .. } => {
                        self.members.returned_calls.matching(predicate)
                    }
                    EventPredicate::MemberRead { .. } => {
                        self.members.returned_reads.matching(predicate)
                    }
                    _ => unreachable!(),
                }
            }
            (EventPredicate::MemberCall { member }, SubjectConstraint::InstanceOf { .. }) => self
                .members
                .instance_calls
                .matching(|key| match &clause.identity {
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
                }),
            _ => None,
        }
    }

    fn occurrences_for_event<'a>(
        &'a self,
        clause: &'a QueryClause,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
        names: &NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
        let view = self.build_event_view(&clause.event, overlay);
        view.resolve(&clause.identity, &clause.event, names)
    }

    fn build_event_view<'a>(
        &'a self,
        event: &EventPredicate,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> EventIndexView<'a> {
        let env = &self.environment;
        match event {
            EventPredicate::Call => EventIndexView {
                name_any: Some(&self.call_indexes.calls),
                module: Some(&self.call_indexes.module_calls),
                global: Some(&self.call_indexes.global_calls),
                module_overlay: overlay.map(|o| &o.module_calls),
                global_overlay: overlay.map(|o| &o.global_calls),
                masked: overlay.map(|o| &o.masked),
                ..EventIndexView::base(env)
            },
            EventPredicate::MemberCall { .. } => EventIndexView {
                path_any: Some(&self.members.calls),
                module: Some(&self.members.module_calls),
                rooted: Some(&self.members.rooted_calls),
                module_overlay: overlay.map(|o| &o.member_calls),
                masked: overlay.map(|o| &o.masked),
                ..EventIndexView::base(env)
            },
            EventPredicate::MemberRead { .. } => EventIndexView {
                path_any: Some(&self.members.reads),
                module: Some(&self.members.module_reads),
                rooted: Some(&self.members.rooted_reads),
                module_overlay: overlay.map(|o| &o.member_reads),
                masked: overlay.map(|o| &o.masked),
                ..EventIndexView::base(env)
            },
            EventPredicate::ClassReference => EventIndexView {
                string_any: Some(&self.constructions.classes),
                module: Some(&self.constructions.module_classes),
                module_overlay: overlay.map(|o| &o.module_classes),
                masked: overlay.map(|o| &o.masked),
                ..EventIndexView::base(env)
            },
            EventPredicate::Construct => EventIndexView {
                name_any: Some(&self.constructions.constructors),
                string_any: Some(&self.constructions.global_constructors),
                module: Some(&self.constructions.module_constructors),
                global: Some(&self.constructions.global_constructors),
                module_overlay: overlay.map(|o| &o.module_constructors),
                masked: overlay.map(|o| &o.masked),
                ..EventIndexView::base(env)
            },
            EventPredicate::Import => EventIndexView {
                literal: Some(&self.literals.imports),
                ..EventIndexView::base(env)
            },
            EventPredicate::StringReference => EventIndexView {
                literal: Some(&self.literals.strings),
                ..EventIndexView::base(env)
            },
        }
    }

    #[cfg(test)]
    pub(super) fn record(&mut self, kind: MatchKind, symbol: impl Into<SmolStr>, span: ByteRange) {
        let symbol = symbol.into();
        match kind {
            MatchKind::Call => {
                let name = self.test_name(symbol.as_str());
                self.call_indexes.calls.push(name, FactId(u32::MAX), span);
            }
            MatchKind::MemberCall => {
                let key = symbol
                    .split('.')
                    .map(|segment| self.test_name(segment))
                    .collect::<Vec<_>>();
                self.members.calls.push(
                    glass_lint_datastructures::NamePath::from_ids(key),
                    FactId(u32::MAX),
                    span,
                );
            }
            MatchKind::MemberRead => {
                let key = symbol
                    .split('.')
                    .map(|segment| self.test_name(segment))
                    .collect::<Vec<_>>();
                self.members.reads.push(
                    glass_lint_datastructures::NamePath::from_ids(key),
                    FactId(u32::MAX),
                    span,
                );
            }
            MatchKind::Import => {
                self.literals.imports.push(symbol, FactId(u32::MAX), span);
            }
            MatchKind::StringContains => {
                self.literals.strings.push(symbol, FactId(u32::MAX), span);
            }
            MatchKind::Class => {
                self.constructions
                    .classes
                    .push(symbol, FactId(u32::MAX), span);
            }
            MatchKind::Constructor => {
                let name = self.test_name(symbol.as_str());
                self.constructions
                    .constructors
                    .push(name, FactId(u32::MAX), span);
            }
            MatchKind::CallArgument => {}
        }
    }
}
