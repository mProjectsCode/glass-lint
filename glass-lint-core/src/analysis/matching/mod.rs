use std::collections::BTreeMap;

#[cfg(test)]
use glass_lint_datastructures::{NamePath, NameTable};
use smol_str::SmolStr;

use crate::{
    analysis::{DerivedPhaseAvailability, project::model::ExportResolution},
    api::{classification::ClassificationEvidence, rule::MatchKind},
};

pub(super) mod evidence;
mod occurrence;
pub(in crate::analysis) use occurrence::ModuleExportKey;
use occurrence::{
    BorrowedOccurrenceIter, ModuleOccurrences, Occurrence, OccurrenceSelection, Occurrences,
    PackageKeyPredicate, PackageOverlay,
};
mod identity_map;
mod indexes;
pub(in crate::analysis) use identity_map::{ModuleIdentityContributions, ModuleIdentityMap};
mod arguments;
pub(in crate::analysis) use arguments::{
    ConstrainedRootInput, MatcherArtifact, MatcherOverlayPolicy, MatcherProjectContext,
    MatcherProjectOverlay, try_compute_constrained_evidence,
};
mod build;
mod query;
use evidence::EvidenceGroup;
pub use evidence::display_span;
pub(in crate::analysis) use query::IndexedRootIter;

#[derive(Debug, Default)]
pub struct OccurrenceIndexes {
    availability: DerivedPhaseAvailability,
    environment: crate::Environment,
    call_indexes: indexes::CallIndexes,
    members: indexes::MemberIndexes,
    constructions: indexes::ConstructionIndexes,
    literals: indexes::LiteralIndexes,
}

type BorrowedModuleBuckets<'a> = BTreeMap<ModuleExportKey, Vec<&'a [Occurrence]>>;
type BorrowedGlobalBuckets<'a> = BTreeMap<SmolStr, Vec<&'a [Occurrence]>>;

/// Which module overlay bucket an event view consults for linked
/// occurrences.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis) enum ModuleOverlayKind {
    Call,
    MemberCall,
    MemberRead,
    Class,
    Constructor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GlobalPromotion {
    Allowed,
    Disabled,
}

#[derive(Clone, Copy, Debug)]
struct ModuleOverlaySource<'a> {
    occurrences: &'a ModuleOccurrences,
    kind: ModuleOverlayKind,
    global_promotion: GlobalPromotion,
}

#[derive(Clone, Debug, Default)]
struct ModuleOccurrenceOverlay<'a> {
    masked: std::collections::BTreeSet<ModuleExportKey>,
    buckets: BTreeMap<ModuleOverlayKind, BorrowedModuleBuckets<'a>>,
}

#[derive(Clone, Debug, Default)]
pub(in crate::analysis) struct LinkedOccurrenceView<'a> {
    module: ModuleOccurrenceOverlay<'a>,
    global_calls: BorrowedGlobalBuckets<'a>,
}

impl<'a> ModuleOccurrenceOverlay<'a> {
    fn remap(
        &mut self,
        source: ModuleOverlaySource<'a>,
        identities: &ModuleIdentityMap,
    ) -> (usize, Vec<(SmolStr, &'a [Occurrence])>) {
        let mut operations = 0usize;
        let mut globals = Vec::new();
        source.occurrences.for_each_bucket(|key, occurrences| {
            operations = operations.saturating_add(1);
            let Some(identity) = LinkedOccurrenceView::identity_for(identities, key) else {
                return;
            };
            self.masked.insert(key.clone());
            match identity {
                ExportResolution::External { module, export } => {
                    self.buckets_mut(source.kind)
                        .entry(ModuleExportKey::new(module, export))
                        .or_default()
                        .push(occurrences);
                }
                ExportResolution::Global { name }
                    if source.global_promotion == GlobalPromotion::Allowed =>
                {
                    globals.push((name, occurrences));
                }
                ExportResolution::Global { .. }
                | ExportResolution::Qualified { .. }
                | ExportResolution::StaticString { .. }
                | ExportResolution::Ambiguous
                | ExportResolution::Unknown => {}
            }
        });
        (operations, globals)
    }

    fn buckets_mut(&mut self, kind: ModuleOverlayKind) -> &mut BorrowedModuleBuckets<'a> {
        self.buckets.entry(kind).or_default()
    }

    fn buckets(&self, kind: ModuleOverlayKind) -> Option<&BorrowedModuleBuckets<'a>> {
        self.buckets.get(&kind)
    }

    fn resolve_module(
        &'a self,
        kind: ModuleOverlayKind,
        base: &'a ModuleOccurrences,
        key: &ModuleExportKey,
    ) -> Option<OccurrenceSelection<'a>> {
        if let Some(slices) = self.buckets(kind).and_then(|buckets| buckets.get(key)) {
            return Some(OccurrenceSelection::Borrowed(BorrowedOccurrenceIter::new(
                None,
                slices.as_slice(),
            )));
        }
        if !self.masked.contains(key) {
            return base.get(key).map(OccurrenceSelection::indexed);
        }
        None
    }

    fn resolve_package(
        &'a self,
        kind: ModuleOverlayKind,
        base: &'a ModuleOccurrences,
        predicate: PackageKeyPredicate<'a>,
    ) -> OccurrenceSelection<'a> {
        match self.buckets(kind) {
            Some(buckets) => {
                let overlay = PackageOverlay::new(&self.masked, buckets);
                OccurrenceSelection::BorrowedPackage(
                    base.package_candidates_with_overlay(predicate, overlay),
                )
            }
            None => OccurrenceSelection::BorrowedPackage(base.package_candidates(predicate)),
        }
    }
}

impl<'a> LinkedOccurrenceView<'a> {
    /// Build the linked occurrence overlay from the module identities.
    ///
    /// Identity remapping, masking, and global promotion belong to this view;
    /// callers receive only the completed overlay and its bounded operation
    /// count.
    pub(in crate::analysis) fn build(
        indexes: &'a OccurrenceIndexes,
        identities: &ModuleIdentityMap,
    ) -> (Self, usize) {
        let mut view = Self::default();
        let mut operations = 0usize;
        for source in [
            ModuleOverlaySource {
                occurrences: indexes.call_indexes.module_calls(),
                kind: ModuleOverlayKind::Call,
                global_promotion: GlobalPromotion::Allowed,
            },
            ModuleOverlaySource {
                occurrences: indexes.members.module_calls(),
                kind: ModuleOverlayKind::MemberCall,
                global_promotion: GlobalPromotion::Disabled,
            },
            ModuleOverlaySource {
                occurrences: indexes.members.module_reads(),
                kind: ModuleOverlayKind::MemberRead,
                global_promotion: GlobalPromotion::Disabled,
            },
            ModuleOverlaySource {
                occurrences: indexes.constructions.module_classes(),
                kind: ModuleOverlayKind::Class,
                global_promotion: GlobalPromotion::Disabled,
            },
            ModuleOverlaySource {
                occurrences: indexes.constructions.module_constructors(),
                kind: ModuleOverlayKind::Constructor,
                global_promotion: GlobalPromotion::Disabled,
            },
        ] {
            let (count, globals) = view.module.remap(source, identities);
            operations += count;
            for (name, occurrences) in globals {
                view.global_calls.entry(name).or_default().push(occurrences);
            }
        }
        (view, operations)
    }

    fn identity_for(
        identities: &ModuleIdentityMap,
        key: &ModuleExportKey,
    ) -> Option<ExportResolution> {
        identities.get(key).cloned().or_else(|| {
            identities
                .get(&ModuleExportKey::wildcard(key.module().clone()))
                .map(|identity| match identity {
                    ExportResolution::External { module, .. } => ExportResolution::External {
                        module: module.clone(),
                        export: key.export().to_owned(),
                    },
                    other => other.clone(),
                })
        })
    }

    pub(in crate::analysis) fn resolve_module(
        &'a self,
        kind: ModuleOverlayKind,
        base: &'a ModuleOccurrences,
        key: &ModuleExportKey,
    ) -> Option<OccurrenceSelection<'a>> {
        self.module.resolve_module(kind, base, key)
    }

    pub(in crate::analysis) fn resolve_global(
        &'a self,
        base: &'a Occurrences,
        name: &SmolStr,
    ) -> Option<OccurrenceSelection<'a>> {
        let base_slice = base.get(name);
        let overlay_slices = self.global_calls.get(name);
        match (base_slice, overlay_slices) {
            (Some(base_slice), Some(overlay_slices)) => Some(OccurrenceSelection::Borrowed(
                BorrowedOccurrenceIter::new(Some(base_slice), overlay_slices.as_slice()),
            )),
            (Some(slice), None) => Some(OccurrenceSelection::indexed(slice)),
            (None, Some(slices)) => Some(OccurrenceSelection::Borrowed(
                BorrowedOccurrenceIter::new(None, slices.as_slice()),
            )),
            (None, None) => None,
        }
    }

    pub(in crate::analysis) fn resolve_package(
        &'a self,
        kind: ModuleOverlayKind,
        base: &'a ModuleOccurrences,
        predicate: PackageKeyPredicate<'a>,
    ) -> OccurrenceSelection<'a> {
        self.module.resolve_package(kind, base, predicate)
    }
}

impl OccurrenceIndexes {
    pub(in crate::analysis) fn with_environment(
        environment: &crate::Environment,
        availability: DerivedPhaseAvailability,
    ) -> Self {
        Self {
            availability,
            environment: environment.clone(),
            ..Self::default()
        }
    }

    pub(in crate::analysis) fn is_available(&self) -> bool {
        self.availability.is_enabled()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn is_empty(&self) -> bool {
        self.call_indexes.is_empty()
            && self.members.is_empty()
            && self.constructions.is_empty()
            && self.literals.is_empty()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_call(&self, names: &NameTable, name: &str) -> bool {
        names
            .lookup(name)
            .is_some_and(|id| self.call_indexes.calls().get(&id).is_some())
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_import(&self, module: &str) -> bool {
        self.literals.imports().get(module).is_some()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_string(&self, value: &str) -> bool {
        self.literals.strings().get(value).is_some()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_any_class(&self) -> bool {
        !self.constructions.classes().is_empty()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_module_class(&self, module: &str, name: &str) -> bool {
        self.constructions
            .module_classes()
            .get(&ModuleExportKey::new(module, name))
            .is_some()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_module_constructor(&self, module: &str, name: &str) -> bool {
        self.constructions
            .module_constructors()
            .get(&ModuleExportKey::new(module, name))
            .is_some()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_constructor(&self, names: &NameTable, name: &str) -> bool {
        names
            .lookup(name)
            .is_some_and(|id| self.constructions.constructors().get(&id).is_some())
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_member_call(&self, names: &NameTable, chain: &str) -> bool {
        let path = chain
            .split('.')
            .filter_map(|segment| names.lookup(segment))
            .collect::<Vec<_>>();
        self.members
            .calls()
            .get(&NamePath::from_ids(path))
            .is_some()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_any_member_call(&self) -> bool {
        !self.members.calls().is_empty()
            || !self.members.rooted_calls().is_empty()
            || !self.members.module_calls().is_empty()
    }
}

pub(super) fn push_owned_evidence(
    evidence: &mut Vec<ClassificationEvidence>,
    kind: MatchKind,
    symbol: String,
    occurrences: OccurrenceSelection<'_>,
) {
    if let Some(item) =
        EvidenceGroup::definite_classification(kind, symbol, occurrences.into_ordered())
    {
        evidence.push(item);
    }
}

#[cfg(test)]
mod tests;
