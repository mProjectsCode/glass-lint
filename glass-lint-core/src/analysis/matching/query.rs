//! Compositional ordinary-clause execution over semantic occurrence indexes.

use std::collections::BTreeSet;

#[cfg(test)]
use smol_str::SmolStr;
use smol_str::ToSmolStr;

use crate::{
    analysis::{
        matching::{
            ClassificationEvidence, ModuleExportKey, ModuleOccurrenceOverlay, Occurrence,
            OccurrenceIndexes,
            occurrence::{ModuleOccurrences, OccurrenceIndex},
            push_owned_evidence,
        },
        value::NamePath,
    },
    api::compiler::rule::{
        EventPredicate, IdentityConstraint, QueryClause, QueryPlan, SubjectConstraint,
    },
};

fn merge_occurrences(
    base: Option<&Vec<Occurrence>>,
    overlay: Option<&Vec<Occurrence>>,
) -> Option<Vec<Occurrence>> {
    let mut merged = base
        .into_iter()
        .flatten()
        .chain(overlay.into_iter().flatten())
        .copied()
        .collect::<Vec<_>>();
    merged.sort_by_key(|occurrence| (occurrence.event(), occurrence.span()));
    merged.dedup();
    (!merged.is_empty()).then_some(merged)
}

fn module_occurrences<K: Ord>(
    base: &OccurrenceIndex<K>,
    overlay: Option<&OccurrenceIndex<K>>,
    masked: bool,
    key: &K,
) -> Option<Vec<Occurrence>> {
    overlay
        .and_then(|overlay| overlay.get(key).cloned())
        .or_else(|| (!masked).then(|| base.get(key).cloned()).flatten())
}

fn package_occurrences(
    base: &ModuleOccurrences,
    overlay: Option<&ModuleOccurrences>,
    masked: Option<&BTreeSet<ModuleExportKey>>,
    mut matches: impl FnMut(&ModuleExportKey) -> bool,
) -> Option<Vec<Occurrence>> {
    let base =
        base.matching(|key| matches(key) && masked.is_none_or(|masked| !masked.contains(key)));
    let overlay = overlay.and_then(|overlay| overlay.matching(|key| matches(key)));
    merge_occurrences(base.as_ref(), overlay.as_ref())
}

impl OccurrenceIndexes {
    /// Execute every ordinary clause through the shared event indexes.
    #[cfg(test)]
    pub(in crate::analysis) fn evidence_for(
        &self,
        plan: &QueryPlan,
    ) -> Vec<ClassificationEvidence> {
        self.evidence_for_with_overlay(plan, None, &self.test_names)
    }

    pub(in crate::analysis) fn evidence_for_with_overlay(
        &self,
        plan: &QueryPlan,
        overlay: Option<&ModuleOccurrenceOverlay>,
        names: &crate::analysis::name::NameTable,
    ) -> Vec<ClassificationEvidence> {
        let mut evidence = Vec::new();
        for clause in plan.clauses() {
            if !clause.constraints.is_empty() {
                continue;
            }
            let occurrences = self.occurrences_for_clause(clause, overlay, names);
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

    pub(in crate::analysis) fn occurrences_for_clause(
        &self,
        clause: &QueryClause,
        overlay: Option<&ModuleOccurrenceOverlay>,
        names: &crate::analysis::name::NameTable,
    ) -> Option<Vec<Occurrence>> {
        if !matches!(clause.subject, SubjectConstraint::Direct) {
            return self.occurrences_for_subject(clause, overlay, names);
        }
        self.occurrences_for_event(clause, overlay, names)
    }

    fn occurrences_for_subject(
        &self,
        clause: &QueryClause,
        _overlay: Option<&ModuleOccurrenceOverlay>,
        names: &crate::analysis::name::NameTable,
    ) -> Option<Vec<Occurrence>> {
        match (&clause.event, &clause.subject) {
            (EventPredicate::MemberCall { member }, SubjectConstraint::ReturnedFrom { .. }) => {
                self.members.returned_calls.matching(|key| {
                    key.source().to_symbol_path(names).is_some_and(|source| {
                        clause
                            .identity
                            .root_or_descendant_matches(&source, &self.environment)
                    }) && crate::analysis::value::NamePath::from_symbol_path(member, names)
                        .is_some_and(|member| member == *key.member())
                })
            }
            (EventPredicate::MemberRead { member }, SubjectConstraint::ReturnedFrom { .. }) => {
                self.members.returned_reads.matching(|key| {
                    key.source().to_symbol_path(names).is_some_and(|source| {
                        clause
                            .identity
                            .root_or_descendant_matches(&source, &self.environment)
                    }) && crate::analysis::value::NamePath::from_symbol_path(member, names)
                        .is_some_and(|member| member == *key.member())
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
    fn occurrences_for_event(
        &self,
        clause: &QueryClause,
        overlay: Option<&ModuleOccurrenceOverlay>,
        names: &crate::analysis::name::NameTable,
    ) -> Option<Vec<Occurrence>> {
        match &clause.event {
            EventPredicate::Call => match &clause.identity {
                IdentityConstraint::Any { name, .. } => names
                    .lookup(name)
                    .and_then(|id| self.call_indexes.calls.get(&id).cloned()),
                IdentityConstraint::Global { name, .. } => merge_occurrences(
                    self.call_indexes.global_calls.get(name),
                    overlay.and_then(|overlay| overlay.call_indexes.global_calls.get(name)),
                ),
                IdentityConstraint::ModuleExport { module, export } => {
                    let key = ModuleExportKey::new(module.clone(), export.clone());
                    module_occurrences(
                        &self.call_indexes.module_calls,
                        overlay.map(|overlay| &overlay.call_indexes.module_calls),
                        overlay.is_some_and(|overlay| overlay.masked.contains(&key)),
                        &key,
                    )
                }
                IdentityConstraint::PackageModuleExport { module, export } => package_occurrences(
                    &self.call_indexes.module_calls,
                    overlay.map(|overlay| &overlay.call_indexes.module_calls),
                    overlay.map(|overlay| &overlay.masked),
                    |key| module.matches(key.module()) && key.export() == export,
                ),
                _ => None,
            },
            EventPredicate::MemberCall { member } => match &clause.identity {
                IdentityConstraint::Any { .. } => {
                    crate::analysis::value::NamePath::from_symbol_path(member, names)
                        .and_then(|member| self.members.calls.get(&member).cloned())
                }
                IdentityConstraint::Rooted { path } => {
                    let expected = NamePath::from_symbol_path(path, names)?;
                    self.members.rooted_calls.matching(|key| {
                        expected.matches_global_object_alias_with(key, names, &self.environment)
                    })
                }
                IdentityConstraint::ModuleNamespace { module } => {
                    let key = ModuleExportKey::new(module.clone(), member.to_string());
                    module_occurrences(
                        &self.members.module_calls,
                        overlay.map(|overlay| &overlay.member_calls),
                        overlay.is_some_and(|overlay| overlay.masked.contains(&key)),
                        &key,
                    )
                }
                IdentityConstraint::PackageModuleNamespace { module } => package_occurrences(
                    &self.members.module_calls,
                    overlay.map(|overlay| &overlay.member_calls),
                    overlay.map(|overlay| &overlay.masked),
                    |key| module.matches(key.module()) && member.eq_chain(key.export()),
                ),
                _ => None,
            },
            EventPredicate::MemberRead { member } => match &clause.identity {
                IdentityConstraint::Any { .. } => {
                    crate::analysis::value::NamePath::from_symbol_path(member, names)
                        .and_then(|member| self.members.reads.get(&member).cloned())
                }
                IdentityConstraint::Rooted { path } => {
                    let expected = NamePath::from_symbol_path(path, names)?;
                    self.members.rooted_reads.matching(|key| {
                        expected.matches_global_object_alias_with(key, names, &self.environment)
                    })
                }
                IdentityConstraint::ModuleNamespace { module } => {
                    let key = ModuleExportKey::new(module.clone(), member.to_string());
                    module_occurrences(
                        &self.members.module_reads,
                        overlay.map(|overlay| &overlay.member_reads),
                        overlay.is_some_and(|overlay| overlay.masked.contains(&key)),
                        &key,
                    )
                }
                IdentityConstraint::PackageModuleNamespace { module } => package_occurrences(
                    &self.members.module_reads,
                    overlay.map(|overlay| &overlay.member_reads),
                    overlay.map(|overlay| &overlay.masked),
                    |key| module.matches(key.module()) && member.eq_chain(key.export()),
                ),
                _ => None,
            },
            EventPredicate::ClassReference => match &clause.identity {
                IdentityConstraint::Any { name, .. } => {
                    self.constructions.classes.get(name).cloned()
                }
                IdentityConstraint::ModuleExport { module, export } => {
                    let key = ModuleExportKey::new(module.clone(), export.clone());
                    module_occurrences(
                        &self.constructions.module_classes,
                        overlay.map(|overlay| &overlay.module_classes),
                        overlay.is_some_and(|overlay| overlay.masked.contains(&key)),
                        &key,
                    )
                }
                IdentityConstraint::PackageModuleExport { module, export } => package_occurrences(
                    &self.constructions.module_classes,
                    overlay.map(|overlay| &overlay.module_classes),
                    overlay.map(|overlay| &overlay.masked),
                    |key| module.matches(key.module()) && key.export() == export,
                ),
                _ => None,
            },
            EventPredicate::Construct => match &clause.identity {
                IdentityConstraint::Any { name, .. } => names
                    .lookup(name)
                    .and_then(|id| self.constructions.constructors.get(&id).cloned()),
                IdentityConstraint::Global { name, .. } => {
                    self.constructions.global_constructors.get(name).cloned()
                }
                IdentityConstraint::ModuleExport { module, export } => {
                    let key = ModuleExportKey::new(module.clone(), export.clone());
                    module_occurrences(
                        &self.constructions.module_constructors,
                        overlay.map(|overlay| &overlay.module_constructors),
                        overlay.is_some_and(|overlay| overlay.masked.contains(&key)),
                        &key,
                    )
                }
                IdentityConstraint::PackageModuleExport { module, export } => package_occurrences(
                    &self.constructions.module_constructors,
                    overlay.map(|overlay| &overlay.module_constructors),
                    overlay.map(|overlay| &overlay.masked),
                    |key| module.matches(key.module()) && key.export() == export,
                ),
                _ => None,
            },
            EventPredicate::Import => match &clause.identity {
                IdentityConstraint::LiteralString { predicate } => {
                    self.literals.imports.get(&predicate.to_smolstr()).cloned()
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
        symbol: impl Into<SmolStr>,
        span: crate::ByteRange,
    ) {
        use crate::analysis::facts::FactId;
        let symbol = symbol.into();
        match kind {
            crate::api::classification::MatchKind::Call => {
                let name = self.test_name(symbol.as_str());
                self.call_indexes.calls.push(name, FactId(u32::MAX), span);
            }
            crate::api::classification::MatchKind::MemberCall => {
                let key = symbol
                    .split('.')
                    .map(|segment| self.test_name(segment))
                    .collect::<Vec<_>>();
                self.members.calls.push(
                    crate::analysis::value::NamePath::from_ids(key),
                    FactId(u32::MAX),
                    span,
                );
            }
            crate::api::classification::MatchKind::MemberRead => {
                let key = symbol
                    .split('.')
                    .map(|segment| self.test_name(segment))
                    .collect::<Vec<_>>();
                self.members.reads.push(
                    crate::analysis::value::NamePath::from_ids(key),
                    FactId(u32::MAX),
                    span,
                );
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
            crate::api::classification::MatchKind::Constructor => {
                let name = self.test_name(symbol.as_str());
                self.constructions
                    .constructors
                    .push(name, FactId(u32::MAX), span);
            }
            crate::api::classification::MatchKind::CallArgument => {}
        }
    }
}
