use std::collections::BTreeMap;

use smol_str::SmolStr;

use crate::{
    analysis::project::model::ExportResolution,
    api::classification::{ClassificationEvidence, MatchKind},
};

#[cfg(test)]
use glass_lint_datastructures::NamePath;

mod occurrence;
pub(in crate::analysis) use occurrence::ModuleExportKey;
use occurrence::{CandidateOccurrences, ModuleOccurrences, Occurrence};
mod indexes;
mod identity_map;
pub(in crate::analysis) use identity_map::ModuleIdentityMap;
mod arguments;
pub(in crate::analysis) use arguments::compute_constrained_evidence_from_stream_with_overlay;
mod build;
mod query;

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
    pub(super) global_calls: BorrowedGlobalBuckets<'a>,
    pub(super) module_calls: BorrowedModuleBuckets<'a>,
    pub(super) member_calls: BorrowedModuleBuckets<'a>,
    pub(super) member_reads: BorrowedModuleBuckets<'a>,
    pub(super) module_classes: BorrowedModuleBuckets<'a>,
    pub(super) module_constructors: BorrowedModuleBuckets<'a>,
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
            .is_some_and(|id| self.call_indexes.calls.get(&id).is_some())
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_import(&self, module: &str) -> bool {
        self.literals.imports.get(module).is_some()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_string(&self, value: &str) -> bool {
        self.literals.strings.get(value).is_some()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_any_class(&self) -> bool {
        !self.constructions.classes.is_empty()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_module_class(&self, module: &str, name: &str) -> bool {
        self.constructions
            .module_classes
            .get(&ModuleExportKey::new(module, name))
            .is_some()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_module_constructor(&self, module: &str, name: &str) -> bool {
        self.constructions
            .module_constructors
            .get(&ModuleExportKey::new(module, name))
            .is_some()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_constructor(&self, name: &str) -> bool {
        self.test_names
            .lookup(name)
            .is_some_and(|id| self.constructions.constructors.get(&id).is_some())
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_member_call(&self, chain: &str) -> bool {
        let path = chain
            .split('.')
            .filter_map(|segment| self.test_names.lookup(segment))
            .collect::<Vec<_>>();
        self.members.calls.get(&NamePath::from_ids(path)).is_some()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_any_member_call(&self) -> bool {
        !self.members.calls.is_empty()
            || !self.members.rooted_calls.is_empty()
            || !self.members.module_calls.is_empty()
    }

    pub(in crate::analysis) fn module_overlay<'a>(
        &'a self,
        identities: &ModuleIdentityMap,
    ) -> LinkedOccurrenceView<'a> {
        let mut overlay = LinkedOccurrenceView::default();
        let identity_for = |key: &ModuleExportKey| {
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
        };

        let mut remap_occurrences =
            |source: &'a ModuleOccurrences,
             target: &mut BorrowedModuleBuckets<'a>,
             mut global_target: Option<&mut BorrowedGlobalBuckets<'a>>| {
                for (key, occurrences) in source.iter() {
                    let Some(identity) = identity_for(key) else {
                        continue;
                    };
                    overlay.masked.insert(key.clone());
                    match identity {
                        ExportResolution::External { module, export } => {
                            target
                                .entry(ModuleExportKey::new(module, export))
                                .or_default()
                                .push(occurrences);
                        }
                        ExportResolution::Global { name } => {
                            if let Some(global_target) = global_target.as_deref_mut() {
                                global_target.entry(name).or_default().push(occurrences);
                            }
                        }
                        ExportResolution::Qualified { .. }
                        | ExportResolution::StaticString { .. }
                        | ExportResolution::Ambiguous
                        | ExportResolution::Unknown => {}
                    }
                }
            };
        remap_occurrences(
            &self.call_indexes.module_calls,
            &mut overlay.module_calls,
            Some(&mut overlay.global_calls),
        );
        remap_occurrences(&self.members.module_calls, &mut overlay.member_calls, None);
        remap_occurrences(&self.members.module_reads, &mut overlay.member_reads, None);
        remap_occurrences(
            &self.constructions.module_classes,
            &mut overlay.module_classes,
            None,
        );
        remap_occurrences(
            &self.constructions.module_constructors,
            &mut overlay.module_constructors,
            None,
        );
        overlay
    }
}

pub(super) fn push_owned_evidence(
    evidence: &mut Vec<ClassificationEvidence>,
    kind: MatchKind,
    symbol: String,
    occurrences: impl IntoIterator<Item = Occurrence>,
) {
    let occurrences: Vec<_> = occurrences
        .into_iter()
        .map(
            |occurrence| crate::api::classification::ClassificationEvidenceOccurrence {
                span: occurrence.span(),
                fact: Some(occurrence.event().0),
            },
        )
        .collect();
    if occurrences.is_empty() {
        return;
    }
    evidence.push(ClassificationEvidence {
        kind,
        symbol,
        count: u32::try_from(occurrences.len()).unwrap_or(u32::MAX),
        truncated: false,
        occurrences,
        related: Vec::new(),
    });
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::{ByteRange, SymbolPath};

    use super::*;
    use crate::{
        Environment,
        analysis::{
            facts::{FactId, build::build_test_stream},
            matching::occurrence::OccurrenceIndex,
            resolution::Resolver,
        },
        api::{compiler::rule::CompiledMatcherPlan, rule::MatcherDecl},
        parse,
    };

    fn span(start: u32, end: u32) -> ByteRange {
        ByteRange::new(start, end).unwrap()
    }

    #[test]
    fn typed_occurrence_index_is_deduplicated() {
        let mut index = OccurrenceIndex::<SmolStr>::default();
        index.push("fetch".into(), FactId(1), span(5, 11));
        index.push("fetch".into(), FactId(1), span(5, 11));
        index.push("fetch".into(), FactId(2), span(20, 26));
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

        let compiled = CompiledMatcherPlan::compile_decls(&[MatcherDecl::builder()
            .member_call_heuristic("client.request")
            .build()
            .unwrap()])
        .unwrap();
        let evidence = facts.evidence_for(compiled.query());
        let reference = facts
            .members
            .calls
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
        let parsed = parse(src, "stream-index.js").expect("source should parse");
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
            index.literals.imports.get("mod").is_some(),
            "should have 'mod' import"
        );
        assert!(
            index.literals.imports.get("other-mod").is_some(),
            "should have 'other-mod' import"
        );
        assert!(
            index.literals.imports.get("fs").is_some(),
            "should have 'fs' require import"
        );

        assert!(
            index.literals.strings.get("hello world").is_some(),
            "should have 'hello world' string literal"
        );

        assert!(
            index.constructions.classes.get("MyClass").is_some(),
            "should have MyClass class"
        );

        assert!(index.has_constructor("URL"), "should have URL constructor");

        assert!(index.has_call("foo"), "should have foo call");
        assert!(
            index
                .call_indexes
                .module_calls
                .get(&ModuleExportKey::new("mod", "foo"))
                .is_some(),
            "should have foo as module call from 'mod'"
        );
    }
}
