//! Rule-independent indexes built from one semantic model.
//!
//! Collection is intentionally separated from `evidence_for`: after scope
//! predeclaration, one fact-building AST traversal feeds deterministic
//! occurrence maps, then each rule selects from those maps.
//! Constrained clauses and flow evidence remain per-rule because their
//! predicates cannot be represented as a shared physical lookup key.
//!
//! Provenance levels stay in separate indexes so heuristic names cannot be
//! mistaken for rooted or module-identified identities. All shared indexes
//! are normalized before queries run.

use std::collections::BTreeMap;

use smol_str::SmolStr;

use super::{
    facts::{CallArgInfo, FactPayload, FactStream},
    syntax::{SymbolCallProvenance, SymbolMemberProvenance},
};
use crate::{
    analysis::SymbolPath,
    api::classification::{ClassificationEvidence, MatchKind},
};

mod occurrence;
pub(in crate::analysis) use occurrence::ModuleExportKey;
use occurrence::{
    InstanceMemberKey, ModuleOccurrences, Occurrence, OccurrenceIndex, Occurrences,
    ReturnedMemberKey,
};
mod arguments;
mod build;
mod query;

#[derive(Debug, Default)]
/// Matcher-independent occurrence indexes projected from one fact stream.
///
/// The indexes are reusable across rule catalogs; constrained clauses and flow
/// subplans are evaluated from facts because their predicates are not safe to
/// collapse into a simple lookup key.
pub struct OccurrenceIndexes {
    environment: crate::Environment,
    // Each map represents a different confidence/provenance level. Do not
    // collapse these into one index: a global spelling, rooted alias, and
    // imported member have intentionally different matching semantics.
    //
    // The fields are deliberately grouped by semantic family rather than by
    // the order in which facts are emitted. That makes it easier to audit a
    // matcher query against the indexes it is allowed to consume.
    call_indexes: CallIndexes,
    members: MemberIndexes,
    constructions: ConstructionIndexes,
    literals: LiteralIndexes,
}

#[derive(Debug, Default)]
/// Additional project identities layered over an immutable local index.
pub(in crate::analysis) struct ModuleOccurrenceOverlay {
    masked: std::collections::BTreeSet<ModuleExportKey>,
    call_indexes: CallIndexes,
    member_calls: ModuleOccurrences,
    member_reads: ModuleOccurrences,
    module_classes: ModuleOccurrences,
    module_constructors: ModuleOccurrences,
}

#[derive(Debug, Default)]
/// Call occurrences partitioned by confidence/provenance level.
pub(super) struct CallIndexes {
    calls: Occurrences,
    global_calls: Occurrences,
    module_calls: ModuleOccurrences,
}

#[derive(Clone, Debug, Default)]
/// Member call/read occurrences partitioned by rooted and module identity.
pub(super) struct MemberIndexes {
    calls: OccurrenceIndex<SymbolPath>,
    rooted_calls: OccurrenceIndex<SymbolPath>,
    module_calls: ModuleOccurrences,
    reads: OccurrenceIndex<SymbolPath>,
    rooted_reads: OccurrenceIndex<SymbolPath>,
    module_reads: ModuleOccurrences,
    returned_calls: OccurrenceIndex<ReturnedMemberKey>,
    returned_reads: OccurrenceIndex<ReturnedMemberKey>,
    instance_calls: OccurrenceIndex<InstanceMemberKey>,
}

#[derive(Clone, Debug, Default)]
/// Class and constructor occurrences partitioned by provenance.
pub(super) struct ConstructionIndexes {
    classes: Occurrences,
    module_classes: ModuleOccurrences,
    constructors: Occurrences,
    global_constructors: Occurrences,
    module_constructors: ModuleOccurrences,
}

#[derive(Clone, Debug, Default)]
/// Import and static-string occurrence indexes.
pub(super) struct LiteralIndexes {
    imports: Occurrences,
    strings: Occurrences,
}

/// The only identities a linked module overlay exposes to matcher indexes.
/// Qualified local values and unknown values are intentionally not queryable
/// by the external-module matcher vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::analysis) enum LinkedModuleIdentity {
    /// Identity resolved to an external module export.
    External { module: SmolStr, export: SmolStr },
    /// Identity resolved to a configured global callable.
    Global { name: SmolStr },
    /// Qualified internal identity not exposed to external matcher queries.
    Qualified { module: u32, export: SmolStr },
    /// Static string value available to argument predicates.
    StaticString { value: String },
    /// Resolution was ambiguous or unsupported.
    Unknown,
}

pub(in crate::analysis) type ModuleIdentityMap = BTreeMap<ModuleExportKey, LinkedModuleIdentity>;

impl OccurrenceIndexes {
    pub(in crate::analysis) fn with_environment(environment: &crate::Environment) -> Self {
        Self {
            environment: environment.clone(),
            ..Self::default()
        }
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_call(&self, name: &str) -> bool {
        self.call_indexes.calls.get(name).is_some()
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
    pub(in crate::analysis) fn has_constructor(&self, name: &str) -> bool {
        self.constructions.constructors.get(name).is_some()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_member_call(&self, chain: &str) -> bool {
        self.members.calls.get(&SymbolPath::from(chain)).is_some()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn has_any_member_call(&self) -> bool {
        !self.members.calls.is_empty()
            || !self.members.rooted_calls.is_empty()
            || !self.members.module_calls.is_empty()
    }

    pub(in crate::analysis) fn module_overlay(
        &self,
        identities: &ModuleIdentityMap,
    ) -> ModuleOccurrenceOverlay {
        let mut overlay = ModuleOccurrenceOverlay::default();
        let identity_for = |key: &ModuleExportKey| {
            identities.get(key).cloned().or_else(|| {
                identities
                    .get(&ModuleExportKey::wildcard(key.module().clone()))
                    .map(|identity| match identity {
                        LinkedModuleIdentity::External { module, .. } => {
                            LinkedModuleIdentity::External {
                                module: module.clone(),
                                export: key.export().to_owned(),
                            }
                        }
                        other => other.clone(),
                    })
            })
        };

        let mut remap_occurrences =
            |source: &ModuleOccurrences,
             target: &mut ModuleOccurrences,
             mut global_target: Option<&mut Occurrences>| {
                for (key, occurrences) in source.iter() {
                    let Some(identity) = identity_for(key) else {
                        continue;
                    };
                    overlay.masked.insert(key.clone());
                    target_entry(&identity, occurrences, target, global_target.as_deref_mut());
                }
            };
        remap_occurrences(
            &self.call_indexes.module_calls,
            &mut overlay.call_indexes.module_calls,
            Some(&mut overlay.call_indexes.global_calls),
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
        overlay.call_indexes.global_calls.normalize();
        overlay.call_indexes.module_calls.normalize();
        overlay.member_calls.normalize();
        overlay.member_reads.normalize();
        overlay.module_classes.normalize();
        overlay.module_constructors.normalize();
        overlay
    }
}

fn target_entry(
    identity: &LinkedModuleIdentity,
    occurrences: &[Occurrence],
    target: &mut ModuleOccurrences,
    global_target: Option<&mut Occurrences>,
) {
    match identity {
        LinkedModuleIdentity::External { module, export } => {
            let key = ModuleExportKey::new(module.clone(), export.clone());
            for occurrence in occurrences {
                target.push_occurrence(key.clone(), *occurrence);
            }
        }
        LinkedModuleIdentity::Global { name } => {
            if let Some(global_target) = global_target {
                for occurrence in occurrences {
                    global_target.push_occurrence(name.clone(), *occurrence);
                }
            }
        }
        LinkedModuleIdentity::Qualified { .. }
        | LinkedModuleIdentity::StaticString { .. }
        | LinkedModuleIdentity::Unknown => {}
    }
}

pub(super) fn push_owned_evidence(
    evidence: &mut Vec<ClassificationEvidence>,
    kind: MatchKind,
    symbol: String,
    occurrences: Option<Vec<Occurrence>>,
) {
    let Some(occurrences) = occurrences else {
        return;
    };
    if occurrences.is_empty() {
        return;
    }
    let occurrences: Vec<_> = occurrences
        .iter()
        .map(
            |occurrence| crate::api::classification::ClassificationEvidenceOccurrence {
                span: occurrence.span(),
                fact: Some(occurrence.event().0),
            },
        )
        .collect();
    evidence.push(ClassificationEvidence {
        kind,
        symbol,
        count: u32::try_from(occurrences.len()).unwrap_or(u32::MAX),
        evidence_truncated: false,
        occurrences,
        related: Vec::new(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ByteRange,
        analysis::facts::FactId,
        api::rule::{MatcherSet, MemberCallMatcher},
    };

    fn span(start: u32, end: u32) -> ByteRange {
        ByteRange::new(start, end).unwrap()
    }

    #[test]
    fn typed_occurrence_index_is_sorted_and_deduplicated() {
        let mut index = OccurrenceIndex::<SmolStr>::default();
        index.push("fetch".into(), FactId(2), span(20, 26));
        index.push("fetch".into(), FactId(1), span(5, 11));
        index.push("fetch".into(), FactId(1), span(5, 11));
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

        let matcher = MatcherSet::from_matchers(vec![crate::api::rule::Matcher::from(
            MemberCallMatcher::heuristic("client.request"),
        )]);
        let compiled = crate::api::compiler::CompiledMatcherPlan::compile(&matcher);
        let evidence = facts.evidence_for(compiled.query());
        let reference = facts
            .members
            .calls
            .iter()
            .filter(|(symbol, _)| **symbol == SymbolPath::from_chain("client.request"))
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
        use super::super::{facts::build::build_test_stream, resolution::Resolver};

        let src = r#"
            import { foo } from 'mod';
            import { Bar } from 'other-mod';
            class MyClass extends Bar {}
            const x = foo;
            foo();
            x.hello();
            new MyClass();
            const s = "hello world";
            require('fs');
        "#;
        let parsed = crate::parse(src, "stream-index.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let stream = build_test_stream(&parsed.program, &resolver);

        let mut index = OccurrenceIndexes::default();
        index.build_from_stream(&stream);
        index.normalize_occurrences();

        // Imports should have both 'mod' and 'other-mod' from import declarations,
        // and 'fs' from require() call.
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

        // String literal should be indexed.
        assert!(
            index.literals.strings.get("hello world").is_some(),
            "should have 'hello world' string literal"
        );

        // Class declaration should be indexed.
        assert!(
            index.constructions.classes.get("MyClass").is_some(),
            "should have MyClass class"
        );

        // Constructor call should be indexed.
        assert!(
            index.constructions.constructors.get("MyClass").is_some(),
            "should have MyClass constructor"
        );

        // foo() is an identifier call with module provenance.
        assert!(
            index.call_indexes.calls.get("foo").is_some(),
            "should have foo call"
        );
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
