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

use crate::{
    analysis::{
        facts::{CallArgInfo, FactPayload, FactStream},
        syntax::{SymbolCallProvenance, SymbolMemberProvenance},
        value::NamePath,
    },
    api::classification::{ClassificationEvidence, MatchKind},
    project::ModuleId,
};

mod occurrence;
pub(in crate::analysis) use occurrence::ModuleExportKey;
use occurrence::{
    CandidateOccurrences, InstanceMemberKey, ModuleOccurrences, NameOccurrences, Occurrence,
    OccurrenceIndex, Occurrences, ReturnedMemberKey,
};
mod arguments;
pub(in crate::analysis) use arguments::compute_constrained_evidence_from_stream_with_overlay;
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
    #[cfg(test)]
    test_names: crate::analysis::name::NameTable,
}

type BorrowedModuleBuckets<'a> = BTreeMap<ModuleExportKey, Vec<&'a [Occurrence]>>;
type BorrowedGlobalBuckets<'a> = BTreeMap<SmolStr, Vec<&'a [Occurrence]>>;

#[derive(Debug, Default)]
/// Project-link view layered over an immutable local occurrence index.
///
/// It stores only identity remaps, masks, and borrowed bucket slices. The
/// source occurrence values remain owned by the local semantic index.
pub(in crate::analysis) struct LinkedOccurrenceView<'a> {
    masked: std::collections::BTreeSet<ModuleExportKey>,
    global_calls: BorrowedGlobalBuckets<'a>,
    module_calls: BorrowedModuleBuckets<'a>,
    member_calls: BorrowedModuleBuckets<'a>,
    member_reads: BorrowedModuleBuckets<'a>,
    module_classes: BorrowedModuleBuckets<'a>,
    module_constructors: BorrowedModuleBuckets<'a>,
}

#[derive(Debug, Default)]
/// Call occurrences partitioned by confidence/provenance level.
///
/// Each sub-index represents a different resolution provenance. A single call
/// site may appear in more than one index when its identity can be established
/// at multiple confidence levels (e.g. a rooted global alias).
pub(super) struct CallIndexes {
    /// Calls resolved through lexical name lookup (local or imported).
    calls: NameOccurrences,
    /// Calls resolved to a configured global binding.
    global_calls: Occurrences,
    /// Calls resolved to a module export identity.
    module_calls: ModuleOccurrences,
}

impl CallIndexes {
    pub(super) fn normalize(&mut self) {
        self.calls.normalize();
        self.global_calls.normalize();
        self.module_calls.normalize();
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.calls.is_empty() && self.global_calls.is_empty() && self.module_calls.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
/// Member call/read occurrences partitioned by provenance level.
///
/// Member chains are indexed at multiple confidence levels: syntactic (as
/// written), rooted (following aliases to a known global), module-export
/// (resolved through import/export), returned (from a known call result), and
/// instance (on a known superclass).
pub(super) struct MemberIndexes {
    calls: OccurrenceIndex<NamePath>,
    rooted_calls: OccurrenceIndex<NamePath>,
    module_calls: ModuleOccurrences,
    reads: OccurrenceIndex<NamePath>,
    rooted_reads: OccurrenceIndex<NamePath>,
    module_reads: ModuleOccurrences,
    returned_calls: OccurrenceIndex<ReturnedMemberKey>,
    returned_reads: OccurrenceIndex<ReturnedMemberKey>,
    instance_calls: OccurrenceIndex<InstanceMemberKey>,
}

impl MemberIndexes {
    pub(super) fn normalize(&mut self) {
        self.calls.normalize();
        self.rooted_calls.normalize();
        self.module_calls.normalize();
        self.reads.normalize();
        self.rooted_reads.normalize();
        self.module_reads.normalize();
        self.returned_calls.normalize();
        self.returned_reads.normalize();
        self.instance_calls.normalize();
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.calls.is_empty()
            && self.rooted_calls.is_empty()
            && self.module_calls.is_empty()
            && self.reads.is_empty()
            && self.rooted_reads.is_empty()
            && self.module_reads.is_empty()
            && self.returned_calls.is_empty()
            && self.returned_reads.is_empty()
            && self.instance_calls.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
/// Class and constructor occurrences partitioned by provenance.
pub(super) struct ConstructionIndexes {
    classes: Occurrences,
    module_classes: ModuleOccurrences,
    constructors: NameOccurrences,
    global_constructors: Occurrences,
    module_constructors: ModuleOccurrences,
}

impl ConstructionIndexes {
    pub(super) fn normalize(&mut self) {
        self.classes.normalize();
        self.module_classes.normalize();
        self.constructors.normalize();
        self.global_constructors.normalize();
        self.module_constructors.normalize();
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.classes.is_empty()
            && self.module_classes.is_empty()
            && self.constructors.is_empty()
            && self.global_constructors.is_empty()
            && self.module_constructors.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
/// Import and static-string occurrence indexes.
pub(super) struct LiteralIndexes {
    imports: Occurrences,
    strings: Occurrences,
}

impl LiteralIndexes {
    pub(super) fn normalize(&mut self) {
        self.imports.normalize();
        self.strings.normalize();
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.imports.is_empty() && self.strings.is_empty()
    }
}

/// The only identities a linked module overlay exposes to matcher indexes.
/// Qualified local values and ambiguous or unknown values are intentionally
/// not queryable by the external-module matcher vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::analysis) enum LinkedModuleIdentity {
    /// Identity resolved to an external module export.
    External { module: SmolStr, export: SmolStr },
    /// Identity resolved to a configured global callable.
    Global { name: SmolStr },
    /// Qualified internal identity not exposed to external matcher queries.
    Qualified { module: ModuleId, export: SmolStr },
    /// Static string value available to argument predicates.
    StaticString { value: String },
    /// Multiple distinct linked paths proved incompatible.
    Ambiguous,
    /// Resolution was unsupported or could not be established.
    Unknown,
}

impl LinkedModuleIdentity {
    /// Convert to a call provenance when this identity maps to an external
    /// module export or a known global. Returns `None` for qualified,
    /// static-string, and unknown identities.
    pub(in crate::analysis) fn to_call_provenance(&self) -> Option<SymbolCallProvenance> {
        match self {
            Self::External { module, export } => Some(SymbolCallProvenance::ModuleExport {
                module: module.clone(),
                export: export.clone(),
            }),
            Self::Global { name } => Some(SymbolCallProvenance::Global { name: name.clone() }),
            Self::Qualified { .. }
            | Self::StaticString { .. }
            | Self::Ambiguous
            | Self::Unknown => None,
        }
    }

    /// Return the static string value when this identity is a `StaticString`.
    pub(in crate::analysis) fn static_string_value(&self) -> Option<&str> {
        match self {
            Self::StaticString { value } => Some(value.as_str()),
            _ => None,
        }
    }
}

/// Imported identities indexed by borrowed module/export parts.
///
/// Occurrence indexes retain [`ModuleExportKey`] beside each event. This
/// overlay is queried at high fan-out, so it owns each module/export string
/// once and accepts borrowed lookups thereafter.
#[derive(Clone, Debug, Default)]
pub(in crate::analysis) struct ModuleIdentityMap {
    modules: BTreeMap<SmolStr, BTreeMap<SmolStr, LinkedModuleIdentity>>,
}

impl ModuleIdentityMap {
    pub(in crate::analysis) fn new() -> Self {
        Self::default()
    }

    pub(in crate::analysis) fn get(&self, key: &ModuleExportKey) -> Option<&LinkedModuleIdentity> {
        self.get_parts(key.module(), key.export())
    }

    pub(in crate::analysis) fn get_parts(
        &self,
        module: &str,
        export: &str,
    ) -> Option<&LinkedModuleIdentity> {
        self.modules.get(module)?.get(export)
    }

    pub(in crate::analysis) fn insert(
        &mut self,
        key: ModuleExportKey,
        value: LinkedModuleIdentity,
    ) -> Option<LinkedModuleIdentity> {
        let (module, export) = key.into_parts();
        self.modules
            .entry(module)
            .or_default()
            .insert(export, value)
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
    fn test_name(&mut self, name: &str) -> crate::analysis::name::NameId {
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
            |source: &'a ModuleOccurrences,
             target: &mut BorrowedModuleBuckets<'a>,
             mut global_target: Option<&mut BorrowedGlobalBuckets<'a>>| {
                for (key, occurrences) in source.iter() {
                    let Some(identity) = identity_for(key) else {
                        continue;
                    };
                    overlay.masked.insert(key.clone());
                    match identity {
                        LinkedModuleIdentity::External { module, export } => {
                            target
                                .entry(ModuleExportKey::new(module, export))
                                .or_default()
                                .push(occurrences);
                        }
                        LinkedModuleIdentity::Global { name } => {
                            if let Some(global_target) = global_target.as_deref_mut() {
                                global_target.entry(name).or_default().push(occurrences);
                            }
                        }
                        LinkedModuleIdentity::Qualified { .. }
                        | LinkedModuleIdentity::StaticString { .. }
                        | LinkedModuleIdentity::Ambiguous
                        | LinkedModuleIdentity::Unknown => {}
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
        evidence_truncated: false,
        occurrences,
        related: Vec::new(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ByteRange, Environment,
        analysis::{
            SymbolPath,
            facts::{FactId, build::build_test_stream},
            resolution::Resolver,
        },
        api::{compiler::rule::CompiledMatcherPlan, rule::MatcherDecl},
        parse,
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
                symbol
                    .to_symbol_path(&facts.test_names)
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
            crate::analysis::lowering::SpanNormalizer::for_program(&parsed.program),
        );
        let stream = build_test_stream(&parsed.program, &mut resolver);

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
        assert!(index.has_constructor("URL"), "should have URL constructor");

        // foo() is an identifier call with module provenance.
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
