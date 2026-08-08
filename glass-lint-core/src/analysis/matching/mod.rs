use std::collections::BTreeMap;

#[cfg(test)]
use glass_lint_datastructures::NamePath;
use smol_str::SmolStr;

use crate::{
    analysis::project::model::ExportResolution,
    api::classification::{ClassificationEvidence, MatchKind},
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
    MatcherArtifact, MatcherProjectContext, compute_constrained_evidence,
};
mod build;
mod query;
use evidence::EvidenceGroup;
pub use evidence::display_span;

#[derive(Debug, Default)]
pub struct OccurrenceIndexes {
    environment: crate::Environment,
    call_indexes: indexes::CallIndexes,
    members: indexes::MemberIndexes,
    constructions: indexes::ConstructionIndexes,
    literals: indexes::LiteralIndexes,
    #[cfg(test)]
    test_names: glass_lint_datastructures::NameTable,
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
    pub(in crate::analysis) fn with_environment(environment: &crate::Environment) -> Self {
        Self {
            environment: environment.clone(),
            ..Self::default()
        }
    }

    #[cfg(test)]
    pub(in crate::analysis) fn is_empty(&self) -> bool {
        self.call_indexes.is_empty()
            && self.members.is_empty()
            && self.constructions.is_empty()
            && self.literals.is_empty()
    }

    #[cfg(test)]
    fn test_name(&mut self, name: &str) -> glass_lint_datastructures::NameId {
        self.test_names.intern(name).expect("test name bound")
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_call(&self, name: &str) -> bool {
        self.test_names
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
    pub(in crate::analysis) fn has_constructor(&self, name: &str) -> bool {
        self.test_names
            .lookup(name)
            .is_some_and(|id| self.constructions.constructors().get(&id).is_some())
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_member_call(&self, chain: &str) -> bool {
        let path = chain
            .split('.')
            .filter_map(|segment| self.test_names.lookup(segment))
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
    if let Some(group) = EvidenceGroup::from_occurrences(
        kind,
        symbol,
        crate::project::MatchCertainty::Definite,
        occurrences.into_ordered(),
    ) {
        evidence.push(group.into_classification());
    }
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::{ByteRange, SymbolPath};

    use super::*;
    use crate::{
        Environment,
        analysis::{
            facts::{FactId, build_test_stream},
            matching::occurrence::OccurrenceIndex,
            resolution::Resolver,
        },
        api::{compiler::rule::CompiledMatcherPlan, rule::EventQuery},
    };

    fn span(start: u32, end: u32) -> ByteRange {
        ByteRange::new(start, end).unwrap()
    }

    #[test]
    fn typed_occurrence_index_is_deduplicated() {
        let mut index = OccurrenceIndex::<SmolStr>::default();
        index.push("fetch".into(), FactId::from_test(2), span(20, 26));
        index.push("fetch".into(), FactId::from_test(1), span(5, 11));
        index.push("fetch".into(), FactId::from_test(1), span(5, 11));
        index.normalize();
        assert_eq!(
            index
                .get("fetch")
                .unwrap()
                .iter()
                .map(Occurrence::span)
                .collect::<Vec<_>>(),
            vec![span(5, 11), span(20, 26)]
        );
    }

    #[test]
    fn optimized_member_query_matches_reference_occurrences() {
        let mut facts = OccurrenceIndexes::default();
        facts.record(MatchKind::MemberCall, "client.request", span(30, 44));
        facts.record(MatchKind::MemberCall, "other.request", span(5, 18));
        facts.record(MatchKind::MemberCall, "client.request", span(10, 24));
        facts.normalize_occurrences();

        let compiled =
            CompiledMatcherPlan::compile(&[EventQuery::member_call_heuristic("client.request")
                .unwrap()
                .into_query()])
            .unwrap();
        let evidence = facts.evidence_for(&compiled);
        let reference = facts
            .members
            .calls()
            .iter()
            .filter(|(symbol, _)| {
                facts
                    .test_names
                    .resolve_path(symbol)
                    .is_some_and(|symbol| symbol == SymbolPath::from_chain("client.request"))
            })
            .flat_map(|(_, occurrences)| occurrences.iter().map(Occurrence::span))
            .collect::<Vec<_>>();
        assert_eq!(evidence.len(), 1);
        assert_eq!(
            evidence[0]
                .occurrences()
                .iter()
                .map(crate::api::classification::ClassificationEvidenceOccurrence::span)
                .collect::<Vec<_>>(),
            reference
        );
    }

    #[test]
    fn unknown_namespace_wildcard_masks_base_module_occurrences() {
        let key = ModuleExportKey::new("namespace", "request");
        let mut indexes = OccurrenceIndexes::default();
        indexes.call_indexes.record_module_call(
            key.clone(),
            Occurrence::new(FactId::from_test(1), span(5, 12)),
        );
        indexes.normalize_occurrences();

        let mut identities = ModuleIdentityMap::new();
        identities.insert(
            ModuleExportKey::wildcard("namespace"),
            ExportResolution::Unknown,
        );
        let (view, _) = LinkedOccurrenceView::build(&indexes, &identities);

        assert!(
            view.resolve_module(
                ModuleOverlayKind::Call,
                indexes.call_indexes.module_calls(),
                &key,
            )
            .is_none()
        );
    }

    #[test]
    fn build_from_stream_populates_all_occurrence_indexes() {
        let src = r#"
            import { foo } from 'mod';
            import { Bar } from 'other-mod';
            class MyClass extends Bar {}
            const x = foo;
            foo();
            x.hello();
            new MyClass();
            new URL("https://example.com");
            const s = "hello world";
            require('fs');
        "#;
        let parsed = crate::parse_test_source(src, "stream-index.js").expect("source should parse");
        let mut environment = Environment::default();
        environment
            .add_globals(["URL", "require"])
            .expect("test globals");
        let mut resolver = Resolver::collect_with_environment(
            &parsed.program,
            &environment,
            crate::analysis::lowering::SpanNormalizer::for_program(&parsed.program, src),
        );
        let stream = build_test_stream(&parsed.program, &mut resolver);

        let mut index = OccurrenceIndexes::default();
        index.build_from_stream(&stream);
        index.normalize_occurrences();

        assert!(
            index.literals.imports().get("mod").is_some(),
            "should have 'mod' import"
        );
        assert!(
            index.literals.imports().get("other-mod").is_some(),
            "should have 'other-mod' import"
        );
        assert!(
            index.literals.imports().get("fs").is_some(),
            "should have 'fs' require import"
        );

        assert!(
            index.literals.strings().get("hello world").is_some(),
            "should have 'hello world' string literal"
        );

        assert!(
            index.constructions.classes().get("MyClass").is_some(),
            "should have MyClass class"
        );

        assert!(index.has_constructor("URL"), "should have URL constructor");

        assert!(index.has_call("foo"), "should have foo call");
        assert!(
            index
                .call_indexes
                .module_calls()
                .get(&ModuleExportKey::new("mod", "foo"))
                .is_some(),
            "should have foo as module call from 'mod'"
        );
        assert!(
            index
                .members
                .module_calls()
                .get(&ModuleExportKey::new("mod", "foo"))
                .is_some(),
            "should have foo as member module call from 'mod'"
        );
    }
}
