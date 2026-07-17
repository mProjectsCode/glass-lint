//! Compositional ordinary-clause execution over semantic occurrence indexes.

use super::{ApiEvidence, MatcherFacts, ModuleExportKey, Occurrence, push_owned_evidence};
use crate::api::compiler::rule::{
    EventPredicate, IdentityConstraint, QueryClause, QueryPlan, SubjectConstraint,
};

impl MatcherFacts {
    /// Execute every ordinary clause through the shared event indexes.
    pub(in crate::analysis) fn evidence_for(&self, plan: &QueryPlan) -> Vec<ApiEvidence> {
        let mut evidence = Vec::new();
        for clause in plan.clauses() {
            if !clause.constraints.is_empty() {
                continue;
            }
            let occurrences = self.occurrences_for_clause(clause);
            push_owned_evidence(
                &mut evidence,
                clause.evidence.kind,
                clause.evidence.symbol.clone(),
                occurrences,
            );
        }
        evidence.sort_by_key(|item| {
            let first = item.occurrences.first().map(|occurrence| occurrence.span);
            (first, item.kind, item.symbol.clone())
        });
        evidence
    }

    fn occurrences_for_clause(&self, clause: &QueryClause) -> Option<Vec<Occurrence>> {
        if !matches!(clause.subject, SubjectConstraint::Direct) {
            return self.occurrences_for_subject(clause);
        }
        self.occurrences_for_event(clause)
    }

    fn occurrences_for_subject(&self, clause: &QueryClause) -> Option<Vec<Occurrence>> {
        match (&clause.event, &clause.subject) {
            (EventPredicate::MemberCall { member }, SubjectConstraint::ReturnedFrom { .. }) => self
                .members
                .returned_calls
                .iter()
                .filter(|(key, _)| {
                    root_or_descendant_matches(&clause.identity, key.module())
                        && key.export() == member
                })
                .flat_map(|(_, values)| values.iter().copied())
                .collect::<Vec<_>>()
                .pipe_some(),
            (EventPredicate::MemberRead { member }, SubjectConstraint::ReturnedFrom { .. }) => self
                .members
                .returned_reads
                .iter()
                .filter(|(key, _)| {
                    root_or_descendant_matches(&clause.identity, key.module())
                        && key.export() == member
                })
                .flat_map(|(_, values)| values.iter().copied())
                .collect::<Vec<_>>()
                .pipe_some(),
            (EventPredicate::MemberCall { member }, SubjectConstraint::InstanceOf { .. }) => self
                .members
                .instance_calls
                .iter()
                .filter(|(key, _)| match &clause.identity {
                    IdentityConstraint::ModuleExport {
                        module: expected_module,
                        export: expected_export,
                    } => {
                        key.identity().module() == expected_module
                            && key.identity().export() == expected_export
                            && key.member() == member
                    }
                    _ => false,
                })
                .flat_map(|(_, values)| values.iter().copied())
                .collect::<Vec<_>>()
                .pipe_some(),
            _ => None,
        }
    }

    fn occurrences_for_event(&self, clause: &QueryClause) -> Option<Vec<Occurrence>> {
        match &clause.event {
            EventPredicate::Call => match &clause.identity {
                IdentityConstraint::Any { name, .. } => self.call_indexes.calls.get(name).cloned(),
                IdentityConstraint::Global { name, .. } => {
                    self.call_indexes.global_calls.get(name).cloned()
                }
                IdentityConstraint::ModuleExport { module, export } => self
                    .call_indexes
                    .module_calls
                    .get(&ModuleExportKey::new(module, export))
                    .cloned(),
                _ => None,
            },
            EventPredicate::MemberCall { member } => match &clause.identity {
                IdentityConstraint::Any { .. } => self.members.calls.get(member).cloned(),
                IdentityConstraint::Rooted { path } => self.members.rooted_calls.get(path).cloned(),
                IdentityConstraint::ModuleNamespace { module } => self
                    .members
                    .module_calls
                    .get(&ModuleExportKey::new(module, member))
                    .cloned(),
                _ => None,
            },
            EventPredicate::MemberRead { member } => match &clause.identity {
                IdentityConstraint::Any { .. } => self.members.reads.get(member).cloned(),
                IdentityConstraint::Rooted { path } => self.members.rooted_reads.get(path).cloned(),
                IdentityConstraint::ModuleNamespace { module } => self
                    .members
                    .module_reads
                    .get(&ModuleExportKey::new(module, member))
                    .cloned(),
                _ => None,
            },
            EventPredicate::ClassReference => match &clause.identity {
                IdentityConstraint::Any { name, .. } => {
                    self.constructions.classes.get(name).cloned()
                }
                IdentityConstraint::ModuleExport { module, export } => self
                    .constructions
                    .module_classes
                    .get(&ModuleExportKey::new(module, export))
                    .cloned(),
                _ => None,
            },
            EventPredicate::Construct => match &clause.identity {
                IdentityConstraint::Any { name, .. } | IdentityConstraint::Global { name, .. } => {
                    self.constructions.global_constructors.get(name).cloned()
                }
                IdentityConstraint::ModuleExport { module, export } => self
                    .constructions
                    .module_constructors
                    .get(&ModuleExportKey::new(module, export))
                    .cloned(),
                _ => None,
            },
            EventPredicate::Import => match &clause.identity {
                IdentityConstraint::LiteralString { predicate } => {
                    self.literals.imports.get(predicate).cloned()
                }
                _ => None,
            },
            EventPredicate::StringReference => match &clause.identity {
                IdentityConstraint::LiteralString { predicate } => Some(
                    self.literals
                        .strings
                        .iter()
                        .filter(|(literal, _)| literal.contains(predicate))
                        .flat_map(|(_, values)| values.iter().copied())
                        .collect(),
                ),
                _ => None,
            },
        }
    }

    #[cfg(test)]
    pub(super) fn record(
        &mut self,
        kind: crate::api::classification::ApiMatchKind,
        symbol: impl Into<String>,
        span: crate::ByteRange,
    ) {
        use crate::analysis::facts::FactId;
        let symbol = symbol.into();
        match kind {
            crate::api::classification::ApiMatchKind::Call => {
                self.call_indexes.calls.push(symbol, FactId(u32::MAX), span);
            }
            crate::api::classification::ApiMatchKind::MemberCall => {
                self.members.calls.push(symbol, FactId(u32::MAX), span);
            }
            crate::api::classification::ApiMatchKind::MemberRead => {
                self.members.reads.push(symbol, FactId(u32::MAX), span);
            }
            crate::api::classification::ApiMatchKind::Import => {
                self.literals.imports.push(symbol, FactId(u32::MAX), span);
            }
            crate::api::classification::ApiMatchKind::StringLiteral => {
                self.literals.strings.push(symbol, FactId(u32::MAX), span);
            }
            crate::api::classification::ApiMatchKind::Class => {
                self.constructions
                    .classes
                    .push(symbol, FactId(u32::MAX), span);
            }
            crate::api::classification::ApiMatchKind::Constructor => self
                .constructions
                .constructors
                .push(symbol, FactId(u32::MAX), span),
            crate::api::classification::ApiMatchKind::CallArgument => {}
        }
    }
}

fn root_or_descendant_matches(identity: &IdentityConstraint, source: &str) -> bool {
    matches!(identity, IdentityConstraint::Rooted { path } if source == path || source.starts_with(&format!("{path}.")))
}

trait PipeSome: Sized {
    fn pipe_some(self) -> Option<Self>;
}

impl<T> PipeSome for Vec<T> {
    fn pipe_some(self) -> Option<Self> {
        (!self.is_empty()).then_some(self)
    }
}
