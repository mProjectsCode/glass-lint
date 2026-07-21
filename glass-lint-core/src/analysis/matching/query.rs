//! Compositional ordinary-clause execution over semantic occurrence indexes.

use std::collections::BTreeSet;

#[cfg(test)]
use smol_str::SmolStr;
use smol_str::ToSmolStr;

use crate::{
    analysis::{
        matching::{
            CandidateOccurrences, ClassificationEvidence, ModuleExportKey,
            ModuleOccurrenceOverlay, Occurrence, OccurrenceIndexes,
            occurrence::{MergeOccurrenceIter, ModuleOccurrences, OccurrenceIndex},
            push_owned_evidence,
        },
        value::NamePath,
    },
    api::compiler::rule::{
        EventPredicate, IdentityConstraint, QueryClause, QueryPlan, SubjectConstraint,
    },
};

fn module_occurrences<'a, K: Ord>(
    base: &'a OccurrenceIndex<K>,
    overlay: Option<&'a OccurrenceIndex<K>>,
    masked: bool,
    key: &K,
) -> Option<CandidateOccurrences<'a>> {
    if let Some(overlay_slice) = overlay.and_then(|overlay| overlay.get(key)) {
        return Some(CandidateOccurrences::Indexed(overlay_slice));
    }
    if !masked && let Some(base_slice) = base.get(key) {
        return Some(CandidateOccurrences::Indexed(base_slice));
    }
    None
}

fn package_occurrences<'a>(
    base: &'a ModuleOccurrences,
    overlay: Option<&'a ModuleOccurrences>,
    masked: Option<&'a BTreeSet<ModuleExportKey>>,
    mut matches: impl FnMut(&ModuleExportKey) -> bool,
) -> Option<CandidateOccurrences<'a>> {
    let base_matches = base.matching(|key| {
        matches(key) && masked.is_none_or(|masked| !masked.contains(key))
    });
    let overlay_matches =
        overlay.and_then(|overlay| overlay.matching(|key| matches(key)));
    let mut merged = if let Some(CandidateOccurrences::Scanned(vec)) = base_matches {
        vec
    } else {
        Vec::new()
    };
    if let Some(CandidateOccurrences::Scanned(vec)) = overlay_matches {
        merged.extend(vec);
    }
    if merged.is_empty() {
        return None;
    }
    Some(CandidateOccurrences::Scanned(merged))
}

fn merged_or_indexed<'a>(
    base: Option<&'a [Occurrence]>,
    overlay: Option<&'a [Occurrence]>,
) -> Option<CandidateOccurrences<'a>> {
    match (base, overlay) {
        (Some(base_slice), Some(overlay_slice)) => Some(CandidateOccurrences::Merged(
            MergeOccurrenceIter::new(base_slice, overlay_slice),
        )),
        (Some(slice), None) | (None, Some(slice)) => {
            Some(CandidateOccurrences::Indexed(slice))
        }
        (None, None) => None,
    }
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

    pub(in crate::analysis) fn evidence_for_with_overlay<'a>(
        &'a self,
        plan: &QueryPlan,
        overlay: Option<&'a ModuleOccurrenceOverlay>,
        names: &crate::analysis::name::NameTable,
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
        evidence.sort_by_key(|item| {
            let first = item.occurrences.first().map(|occurrence| occurrence.span);
            (first, item.kind, item.symbol.clone())
        });
        evidence
    }

    pub(in crate::analysis) fn occurrences_for_clause<'a>(
        &'a self,
        clause: &QueryClause,
        overlay: Option<&'a ModuleOccurrenceOverlay>,
        names: &crate::analysis::name::NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
        if !matches!(clause.subject, SubjectConstraint::Direct) {
            return self.occurrences_for_subject(clause, overlay, names);
        }
        self.occurrences_for_event(clause, overlay, names)
    }

    fn occurrences_for_subject<'a>(
        &'a self,
        clause: &QueryClause,
        _overlay: Option<&'a ModuleOccurrenceOverlay>,
        names: &crate::analysis::name::NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
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
    fn occurrences_for_event<'a>(
        &'a self,
        clause: &QueryClause,
        overlay: Option<&'a ModuleOccurrenceOverlay>,
        names: &crate::analysis::name::NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
        match &clause.event {
            EventPredicate::Call => match &clause.identity {
                IdentityConstraint::Any { name, .. } => names
                    .lookup(name)
                    .and_then(|id| self.call_indexes.calls.get(&id))
                    .map(CandidateOccurrences::Indexed),
                IdentityConstraint::Global { name, .. } => merged_or_indexed(
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
                IdentityConstraint::PackageModuleExport { module, export } => {
                    package_occurrences(
                        &self.call_indexes.module_calls,
                        overlay.map(|overlay| &overlay.call_indexes.module_calls),
                        overlay.map(|overlay| &overlay.masked),
                        |key| module.matches(key.module()) && key.export() == export,
                    )
                }
                _ => None,
            },
            EventPredicate::MemberCall { member } => match &clause.identity {
                IdentityConstraint::Any { .. } => {
                    crate::analysis::value::NamePath::from_symbol_path(member, names)
                        .and_then(|member| self.members.calls.get(&member))
                        .map(CandidateOccurrences::Indexed)
                }
                IdentityConstraint::Rooted { path } => {
                    let expected = NamePath::from_symbol_path(path, names)?;
                    self.members.rooted_calls.matching(|key| {
                        expected.matches_global_object_alias_with(
                            key, names, &self.environment,
                        )
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
                IdentityConstraint::PackageModuleNamespace { module } => {
                    package_occurrences(
                        &self.members.module_calls,
                        overlay.map(|overlay| &overlay.member_calls),
                        overlay.map(|overlay| &overlay.masked),
                        |key| module.matches(key.module()) && member.eq_chain(key.export()),
                    )
                }
                _ => None,
            },
            EventPredicate::MemberRead { member } => match &clause.identity {
                IdentityConstraint::Any { .. } => {
                    crate::analysis::value::NamePath::from_symbol_path(member, names)
                        .and_then(|member| self.members.reads.get(&member))
                        .map(CandidateOccurrences::Indexed)
                }
                IdentityConstraint::Rooted { path } => {
                    let expected = NamePath::from_symbol_path(path, names)?;
                    self.members.rooted_reads.matching(|key| {
                        expected.matches_global_object_alias_with(
                            key, names, &self.environment,
                        )
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
                IdentityConstraint::PackageModuleNamespace { module } => {
                    package_occurrences(
                        &self.members.module_reads,
                        overlay.map(|overlay| &overlay.member_reads),
                        overlay.map(|overlay| &overlay.masked),
                        |key| module.matches(key.module()) && member.eq_chain(key.export()),
                    )
                }
                _ => None,
            },
            EventPredicate::ClassReference => match &clause.identity {
                IdentityConstraint::Any { name, .. } => self
                    .constructions
                    .classes
                    .get(name)
                    .map(CandidateOccurrences::Indexed),
                IdentityConstraint::ModuleExport { module, export } => {
                    let key = ModuleExportKey::new(module.clone(), export.clone());
                    module_occurrences(
                        &self.constructions.module_classes,
                        overlay.map(|overlay| &overlay.module_classes),
                        overlay.is_some_and(|overlay| overlay.masked.contains(&key)),
                        &key,
                    )
                }
                IdentityConstraint::PackageModuleExport { module, export } => {
                    package_occurrences(
                        &self.constructions.module_classes,
                        overlay.map(|overlay| &overlay.module_classes),
                        overlay.map(|overlay| &overlay.masked),
                        |key| module.matches(key.module()) && key.export() == export,
                    )
                }
                _ => None,
            },
            EventPredicate::Construct => match &clause.identity {
                IdentityConstraint::Any { name, .. } => names
                    .lookup(name)
                    .and_then(|id| self.constructions.constructors.get(&id))
                    .map(CandidateOccurrences::Indexed),
                IdentityConstraint::Global { name, .. } => self
                    .constructions
                    .global_constructors
                    .get(name)
                    .map(CandidateOccurrences::Indexed),
                IdentityConstraint::ModuleExport { module, export } => {
                    let key = ModuleExportKey::new(module.clone(), export.clone());
                    module_occurrences(
                        &self.constructions.module_constructors,
                        overlay.map(|overlay| &overlay.module_constructors),
                        overlay.is_some_and(|overlay| overlay.masked.contains(&key)),
                        &key,
                    )
                }
                IdentityConstraint::PackageModuleExport { module, export } => {
                    package_occurrences(
                        &self.constructions.module_constructors,
                        overlay.map(|overlay| &overlay.module_constructors),
                        overlay.map(|overlay| &overlay.masked),
                        |key| module.matches(key.module()) && key.export() == export,
                    )
                }
                _ => None,
            },
            EventPredicate::Import => match &clause.identity {
                IdentityConstraint::LiteralString { predicate } => self
                    .literals
                    .imports
                    .get(&predicate.to_smolstr())
                    .map(CandidateOccurrences::Indexed),
                IdentityConstraint::PackageSpecifier { pattern } => {
                    self.literals
                        .imports
                        .matching(|specifier| pattern.matches(specifier))
                }
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
