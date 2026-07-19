//! Compositional ordinary-clause execution over semantic occurrence indexes.

use super::{
    ClassificationEvidence, ModuleExportKey, Occurrence, OccurrenceIndexes, push_owned_evidence,
};
use crate::api::compiler::rule::{
    EventPredicate, IdentityConstraint, QueryClause, QueryPlan, SubjectConstraint,
};

impl OccurrenceIndexes {
    /// Execute every ordinary clause through the shared event indexes.
    pub(in crate::analysis) fn evidence_for(
        &self,
        plan: &QueryPlan,
    ) -> Vec<ClassificationEvidence> {
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
            (EventPredicate::MemberCall { member }, SubjectConstraint::ReturnedFrom { .. }) => {
                self.members.returned_calls.matching(|key| {
                    clause
                        .identity
                        .root_or_descendant_matches(key.source(), &self.environment)
                        && member == key.member()
                })
            }
            (EventPredicate::MemberRead { member }, SubjectConstraint::ReturnedFrom { .. }) => {
                self.members.returned_reads.matching(|key| {
                    clause
                        .identity
                        .root_or_descendant_matches(key.source(), &self.environment)
                        && member == key.member()
                })
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

    #[allow(clippy::too_many_lines)]
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
                IdentityConstraint::PackageModuleExport { module, export } => self
                    .call_indexes
                    .module_calls
                    .matching(|key| module.matches(key.module()) && key.export() == export),
                _ => None,
            },
            EventPredicate::MemberCall { member } => match &clause.identity {
                IdentityConstraint::Any { .. } => self.members.calls.get(member).cloned(),
                IdentityConstraint::Rooted { path } => self
                    .members
                    .rooted_calls
                    .matching(|key| path.matches_global_object_alias(key, &self.environment)),
                IdentityConstraint::ModuleNamespace { module } => self
                    .members
                    .module_calls
                    .get(&ModuleExportKey::new(module, member.to_string()))
                    .cloned(),
                IdentityConstraint::PackageModuleNamespace { module } => self
                    .members
                    .module_calls
                    .matching(|key| module.matches(key.module()) && member.eq_chain(key.export())),
                _ => None,
            },
            EventPredicate::MemberRead { member } => match &clause.identity {
                IdentityConstraint::Any { .. } => self.members.reads.get(member).cloned(),
                IdentityConstraint::Rooted { path } => self
                    .members
                    .rooted_reads
                    .matching(|key| path.matches_global_object_alias(key, &self.environment)),
                IdentityConstraint::ModuleNamespace { module } => self
                    .members
                    .module_reads
                    .get(&ModuleExportKey::new(module, member.to_string()))
                    .cloned(),
                IdentityConstraint::PackageModuleNamespace { module } => self
                    .members
                    .module_reads
                    .matching(|key| module.matches(key.module()) && member.eq_chain(key.export())),
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
                IdentityConstraint::PackageModuleExport { module, export } => self
                    .constructions
                    .module_classes
                    .matching(|key| module.matches(key.module()) && key.export() == export),
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
                IdentityConstraint::PackageModuleExport { module, export } => self
                    .constructions
                    .module_constructors
                    .matching(|key| module.matches(key.module()) && key.export() == export),
                _ => None,
            },
            EventPredicate::Import => match &clause.identity {
                IdentityConstraint::LiteralString { predicate } => {
                    self.literals.imports.get(predicate).cloned()
                }
                IdentityConstraint::PackageSpecifier { pattern } => self
                    .literals
                    .imports
                    .matching(|specifier| pattern.matches(specifier)),
                _ => None,
            },
            EventPredicate::StringReference => match &clause.identity {
                IdentityConstraint::LiteralString { predicate } => self
                    .literals
                    .strings
                    .matching(|literal| literal.contains(predicate)),
                _ => None,
            },
        }
    }

    #[cfg(test)]
    pub(super) fn record(
        &mut self,
        kind: crate::api::classification::MatchKind,
        symbol: impl Into<String>,
        span: crate::ByteRange,
    ) {
        use crate::analysis::facts::FactId;
        let symbol = symbol.into();
        match kind {
            crate::api::classification::MatchKind::Call => {
                self.call_indexes.calls.push(symbol, FactId(u32::MAX), span);
            }
            crate::api::classification::MatchKind::MemberCall => {
                self.members
                    .calls
                    .push(symbol.into(), FactId(u32::MAX), span);
            }
            crate::api::classification::MatchKind::MemberRead => {
                self.members
                    .reads
                    .push(symbol.into(), FactId(u32::MAX), span);
            }
            crate::api::classification::MatchKind::Import => {
                self.literals.imports.push(symbol, FactId(u32::MAX), span);
            }
            crate::api::classification::MatchKind::StringContains => {
                self.literals.strings.push(symbol, FactId(u32::MAX), span);
            }
            crate::api::classification::MatchKind::Class => {
                self.constructions
                    .classes
                    .push(symbol, FactId(u32::MAX), span);
            }
            crate::api::classification::MatchKind::Constructor => self
                .constructions
                .constructors
                .push(symbol, FactId(u32::MAX), span),
            crate::api::classification::MatchKind::CallArgument => {}
        }
    }
}
