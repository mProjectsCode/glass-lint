//! Compositional ordinary-clause execution over semantic occurrence indexes.

use std::collections::{BTreeMap, BTreeSet};

use glass_lint_datastructures::{NamePath, NameTable, SymbolPath};
use smol_str::SmolStr;

use crate::{
    Environment,
    analysis::{
        matching::{
            CandidateOccurrences, ClassificationEvidence, LinkedOccurrenceView, Occurrence,
            OccurrenceIndexes,
            occurrence::{
                BorrowedOccurrenceIter, BorrowedPackageOccurrenceIter, ModuleExportKey,
                ModuleOccurrences, NameOccurrences, OccurrenceIndex, Occurrences,
                PackageKeyPredicate, PackageMatchKind, ReturnedMemberKey,
            },
            push_owned_evidence,
        },
        value::matches_global_object_alias_with,
    },
    api::{
        compiler::rule::{
            EventPredicate, IdentityConstraint, QueryClause, QueryPlan, SubjectConstraint,
        },
        rule::ModuleSpecifierPattern,
    },
};

fn module_occurrences<'a, K: Ord>(
    base: &'a OccurrenceIndex<K>,
    overlay: Option<&'a BTreeMap<K, Vec<&'a [Occurrence]>>>,
    masked: bool,
    key: &K,
) -> Option<CandidateOccurrences<'a>> {
    if let Some(overlay_slices) = overlay.and_then(|overlay| overlay.get(key)) {
        return Some(CandidateOccurrences::Borrowed(BorrowedOccurrenceIter::new(
            None,
            overlay_slices.as_slice(),
        )));
    }
    if !masked && let Some(base_slice) = base.get(key) {
        return Some(CandidateOccurrences::Indexed(base_slice));
    }
    None
}

fn package_occurrences<'a>(
    base: &'a ModuleOccurrences,
    overlay: Option<&'a BTreeMap<ModuleExportKey, Vec<&'a [Occurrence]>>>,
    masked: Option<&'a BTreeSet<ModuleExportKey>>,
    predicate: PackageKeyPredicate<'a>,
) -> CandidateOccurrences<'a> {
    let iter = BorrowedPackageOccurrenceIter::new(predicate, masked, base.as_map(), overlay);
    CandidateOccurrences::BorrowedPackage(iter)
}

fn merged_or_indexed<'a>(
    base: Option<&'a [Occurrence]>,
    overlay: Option<&'a Vec<&'a [Occurrence]>>,
) -> Option<CandidateOccurrences<'a>> {
    match (base, overlay) {
        (Some(base_slice), Some(overlay_slices)) => Some(CandidateOccurrences::Borrowed(
            BorrowedOccurrenceIter::new(Some(base_slice), overlay_slices.as_slice()),
        )),
        (Some(slice), None) => Some(CandidateOccurrences::Indexed(slice)),
        (None, Some(slices)) => Some(CandidateOccurrences::Borrowed(BorrowedOccurrenceIter::new(
            None,
            slices.as_slice(),
        ))),
        (None, None) => None,
    }
}

/// Centralized identity dispatcher that performs identity classification, key
/// creation, masking, and merged exact/package/global iteration once for all
/// event families. Each event adapter provides its family-specific indexes.
struct EventIndexView<'a> {
    name_any: Option<&'a NameOccurrences>,
    string_any: Option<&'a Occurrences>,
    path_any: Option<&'a OccurrenceIndex<NamePath>>,
    module: Option<&'a ModuleOccurrences>,
    global: Option<&'a Occurrences>,
    rooted: Option<&'a OccurrenceIndex<NamePath>>,
    literal: Option<&'a Occurrences>,
    module_overlay: Option<&'a BTreeMap<ModuleExportKey, Vec<&'a [Occurrence]>>>,
    global_overlay: Option<&'a BTreeMap<SmolStr, Vec<&'a [Occurrence]>>>,
    masked: Option<&'a BTreeSet<ModuleExportKey>>,
    environment: &'a Environment,
}

impl<'a> EventIndexView<'a> {
    /// Returns a view with all index fields defaulted to `None`.
    fn base(environment: &'a Environment) -> Self {
        Self {
            name_any: None,
            string_any: None,
            path_any: None,
            module: None,
            global: None,
            rooted: None,
            literal: None,
            module_overlay: None,
            global_overlay: None,
            masked: None,
            environment,
        }
    }

    fn resolve(
        &self,
        identity: &'a IdentityConstraint,
        event: &'a EventPredicate,
        names: &NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
        match identity {
            IdentityConstraint::Any { name, .. } => self.resolve_any(name, event, names),
            IdentityConstraint::Global { name, .. } => self.resolve_global(name),
            IdentityConstraint::ModuleExport { module, export } => {
                self.resolve_module_export(module, export)
            }
            IdentityConstraint::PackageModuleExport { module, export } => {
                self.resolve_package_export(module, export)
            }
            IdentityConstraint::ModuleNamespace { module } => {
                self.resolve_module_namespace(module, event)
            }
            IdentityConstraint::PackageModuleNamespace { module } => {
                self.resolve_package_namespace(module, event)
            }
            IdentityConstraint::Rooted { path } => self.resolve_rooted(path, event, names),
            IdentityConstraint::LiteralString { predicate } => {
                self.resolve_literal(predicate, event)
            }
            IdentityConstraint::PackageSpecifier { pattern } => {
                self.resolve_package_specifier(pattern)
            }
        }
    }

    fn resolve_any(
        &self,
        name: &SmolStr,
        event: &'a EventPredicate,
        names: &NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
        // Try NameOccurrences first (Call, Construct)
        if let Some(name_index) = self.name_any
            && let Some(id) = names.lookup(name)
            && let Some(result) = name_index.get(&id)
        {
            return Some(CandidateOccurrences::Indexed(result));
        }
        // Try OccurrenceIndex<NamePath> (MemberCall, MemberRead)
        if let (
            Some(path_index),
            EventPredicate::MemberCall { member } | EventPredicate::MemberRead { member },
        ) = (self.path_any, event)
            && let Some(path) = names.lookup_path(member)
            && let Some(result) = path_index.get(&path)
        {
            return Some(CandidateOccurrences::Indexed(result));
        }
        // Try string-indexed Occurrences (ClassReference)
        if let Some(string_index) = self.string_any
            && let Some(result) = string_index.get(name.as_str())
        {
            return Some(CandidateOccurrences::Indexed(result));
        }
        None
    }

    fn resolve_global(&self, name: &SmolStr) -> Option<CandidateOccurrences<'a>> {
        merged_or_indexed(
            self.global?.get(name),
            self.global_overlay.and_then(|o| o.get(name)),
        )
    }

    fn resolve_module_export(
        &self,
        module: &SmolStr,
        export: &SmolStr,
    ) -> Option<CandidateOccurrences<'a>> {
        let key = ModuleExportKey::new(module.clone(), export.clone());
        module_occurrences(
            self.module?,
            self.module_overlay,
            self.masked.is_some_and(|masked| masked.contains(&key)),
            &key,
        )
    }

    fn resolve_package_export(
        &self,
        module: &'a ModuleSpecifierPattern,
        export: &'a SmolStr,
    ) -> Option<CandidateOccurrences<'a>> {
        Some(package_occurrences(
            self.module?,
            self.module_overlay,
            self.masked,
            PackageKeyPredicate::new(module, PackageMatchKind::Export(export)),
        ))
    }

    fn resolve_module_namespace(
        &self,
        module: &SmolStr,
        event: &'a EventPredicate,
    ) -> Option<CandidateOccurrences<'a>> {
        let key = match event {
            EventPredicate::MemberCall { member } | EventPredicate::MemberRead { member } => {
                ModuleExportKey::new(module.clone(), member.to_string())
            }
            _ => return None,
        };
        module_occurrences(
            self.module?,
            self.module_overlay,
            self.masked.is_some_and(|masked| masked.contains(&key)),
            &key,
        )
    }

    fn resolve_package_namespace(
        &self,
        module: &'a ModuleSpecifierPattern,
        event: &'a EventPredicate,
    ) -> Option<CandidateOccurrences<'a>> {
        let (EventPredicate::MemberCall { member } | EventPredicate::MemberRead { member }) = event
        else {
            return None;
        };
        Some(package_occurrences(
            self.module?,
            self.module_overlay,
            self.masked,
            PackageKeyPredicate::new(module, PackageMatchKind::Namespace(member)),
        ))
    }

    fn resolve_rooted(
        &self,
        path: &'a SymbolPath,
        event: &'a EventPredicate,
        names: &NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
        let (EventPredicate::MemberCall { member: _ } | EventPredicate::MemberRead { member: _ }) =
            event
        else {
            return None;
        };
        let expected = names.lookup_path(path)?;
        self.rooted?.matching(|key| {
            matches_global_object_alias_with(key, &expected, names, self.environment)
        })
    }

    fn resolve_literal(
        &self,
        predicate: &str,
        event: &EventPredicate,
    ) -> Option<CandidateOccurrences<'a>> {
        match event {
            EventPredicate::Import => self
                .literal?
                .get(&SmolStr::new(predicate))
                .map(CandidateOccurrences::Indexed),
            EventPredicate::StringReference => self
                .literal?
                .matching(|literal| literal.contains(predicate)),
            _ => None,
        }
    }

    fn resolve_package_specifier(
        &self,
        pattern: &ModuleSpecifierPattern,
    ) -> Option<CandidateOccurrences<'a>> {
        self.literal?
            .matching(|specifier| pattern.matches(specifier))
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
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
        names: &glass_lint_datastructures::NameTable,
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
        names: &glass_lint_datastructures::NameTable,
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
        names: &glass_lint_datastructures::NameTable,
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
        names: &glass_lint_datastructures::NameTable,
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
    pub(super) fn record(
        &mut self,
        kind: crate::api::classification::MatchKind,
        symbol: impl Into<SmolStr>,
        span: glass_lint_datastructures::ByteRange,
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
                    glass_lint_datastructures::NamePath::from_ids(key),
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
                    glass_lint_datastructures::NamePath::from_ids(key),
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
