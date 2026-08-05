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
    BorrowedOccurrenceIter, CandidateOccurrences, ModuleOccurrences, Occurrence, Occurrences,
    PackageKeyPredicate, PackageOverlay,
};
mod identity_map;
mod indexes;
pub(in crate::analysis) use identity_map::ModuleIdentityMap;
mod arguments;
pub(in crate::analysis) use arguments::{
    MatcherLocalInput, MatcherProjectOverlay, compute_constrained_evidence,
};
mod build;
mod query;
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

#[derive(Debug, Default)]
pub(in crate::analysis) struct LinkedOccurrenceView<'a> {
    masked: std::collections::BTreeSet<ModuleExportKey>,
    global_calls: BorrowedGlobalBuckets<'a>,
    module_calls: BorrowedModuleBuckets<'a>,
    member_calls: BorrowedModuleBuckets<'a>,
    member_reads: BorrowedModuleBuckets<'a>,
    module_classes: BorrowedModuleBuckets<'a>,
    module_constructors: BorrowedModuleBuckets<'a>,
}

/// Which module overlay bucket an event view consults for linked
/// occurrences.
#[derive(Clone, Copy)]
pub(in crate::analysis) enum ModuleOverlayKind {
    Call,
    MemberCall,
    MemberRead,
    Class,
    Constructor,
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
        operations += view.remap(
            indexes.call_indexes.module_calls(),
            ModuleOverlayKind::Call,
            true,
            identities,
        );
        operations += view.remap(
            indexes.members.module_calls(),
            ModuleOverlayKind::MemberCall,
            false,
            identities,
        );
        operations += view.remap(
            indexes.members.module_reads(),
            ModuleOverlayKind::MemberRead,
            false,
            identities,
        );
        operations += view.remap(
            indexes.constructions.module_classes(),
            ModuleOverlayKind::Class,
            false,
            identities,
        );
        operations += view.remap(
            indexes.constructions.module_constructors(),
            ModuleOverlayKind::Constructor,
            false,
            identities,
        );
        (view, operations)
    }

    fn remap(
        &mut self,
        source: &'a ModuleOccurrences,
        kind: ModuleOverlayKind,
        promote_globals: bool,
        identities: &ModuleIdentityMap,
    ) -> usize {
        let mut operations = 0usize;
        source.for_each_bucket(|key, occurrences| {
            operations = operations.saturating_add(1);
            let Some(identity) = Self::identity_for(identities, key) else {
                return;
            };
            self.masked.insert(key.clone());
            match identity {
                ExportResolution::External { module, export } => {
                    self.module_buckets_mut(kind)
                        .entry(ModuleExportKey::new(module, export))
                        .or_default()
                        .push(occurrences);
                }
                ExportResolution::Global { name } if promote_globals => {
                    self.global_calls.entry(name).or_default().push(occurrences);
                }
                ExportResolution::Global { .. }
                | ExportResolution::Qualified { .. }
                | ExportResolution::StaticString { .. }
                | ExportResolution::Ambiguous
                | ExportResolution::Unknown => {}
            }
        });
        operations
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

    fn module_buckets_mut(
        &mut self,
        kind: ModuleOverlayKind,
    ) -> &mut BTreeMap<ModuleExportKey, Vec<&'a [Occurrence]>> {
        match kind {
            ModuleOverlayKind::Call => &mut self.module_calls,
            ModuleOverlayKind::MemberCall => &mut self.member_calls,
            ModuleOverlayKind::MemberRead => &mut self.member_reads,
            ModuleOverlayKind::Class => &mut self.module_classes,
            ModuleOverlayKind::Constructor => &mut self.module_constructors,
        }
    }

    pub(in crate::analysis) fn resolve_module(
        &'a self,
        kind: ModuleOverlayKind,
        base: &'a ModuleOccurrences,
        key: &ModuleExportKey,
    ) -> Option<CandidateOccurrences<'a>> {
        if let Some(slices) = self.module_buckets(kind).get(key) {
            return Some(CandidateOccurrences::Borrowed(BorrowedOccurrenceIter::new(
                None,
                slices.as_slice(),
            )));
        }
        if !self.masked.contains(key) {
            return base.get(key).map(CandidateOccurrences::Indexed);
        }
        None
    }

    pub(in crate::analysis) fn resolve_global(
        &'a self,
        base: &'a Occurrences,
        name: &SmolStr,
    ) -> Option<CandidateOccurrences<'a>> {
        let base_slice = base.get(name);
        let overlay_slices = self.global_calls.get(name);
        match (base_slice, overlay_slices) {
            (Some(base_slice), Some(overlay_slices)) => Some(CandidateOccurrences::Borrowed(
                BorrowedOccurrenceIter::new(Some(base_slice), overlay_slices.as_slice()),
            )),
            (Some(slice), None) => Some(CandidateOccurrences::Indexed(slice)),
            (None, Some(slices)) => Some(CandidateOccurrences::Borrowed(
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
    ) -> CandidateOccurrences<'a> {
        let overlay = PackageOverlay::new(&self.masked, self.module_buckets(kind));
        CandidateOccurrences::BorrowedPackage(
            base.package_candidates_with_overlay(predicate, overlay),
        )
    }

    fn module_buckets(
        &self,
        kind: ModuleOverlayKind,
    ) -> &BTreeMap<ModuleExportKey, Vec<&'a [Occurrence]>> {
        match kind {
            ModuleOverlayKind::Call => &self.module_calls,
            ModuleOverlayKind::MemberCall => &self.member_calls,
            ModuleOverlayKind::MemberRead => &self.member_reads,
            ModuleOverlayKind::Class => &self.module_classes,
            ModuleOverlayKind::Constructor => &self.module_constructors,
        }
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

pub(super) fn owned_occurrences(
    occurrences: impl IntoIterator<Item = Occurrence>,
) -> Vec<crate::api::classification::ClassificationEvidenceOccurrence> {
    occurrences
        .into_iter()
        .map(
            |occurrence| crate::api::classification::ClassificationEvidenceOccurrence {
                span: occurrence.span(),
                fact: Some(occurrence.event().raw()),
                trace: None,
            },
        )
        .collect()
}

pub(super) fn push_owned_evidence(
    evidence: &mut Vec<ClassificationEvidence>,
    kind: MatchKind,
    symbol: String,
    occurrences: impl IntoIterator<Item = Occurrence>,
) {
    let occurrences = owned_occurrences(occurrences);
    if occurrences.is_empty() {
        return;
    }
    evidence.push(ClassificationEvidence {
        kind,
        symbol,
        count: u32::try_from(occurrences.len()).unwrap_or(u32::MAX),
        truncated: false,
        certainty: crate::project::MatchCertainty::Definite,
        occurrences,
    });
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
                .occurrences
                .iter()
                .map(|occurrence| occurrence.span)
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
