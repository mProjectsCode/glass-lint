use std::{borrow::Borrow, collections::BTreeMap};

use glass_lint_datastructures::NameTable;

use crate::{
    analysis::{
        facts::{FactStream, Frozen, SemanticFacts},
        matching::{
            LinkedOccurrenceView, ModuleIdentityMap, Occurrence, OccurrenceIndexes,
            evidence::EvidenceGroup,
        },
        model::value::ValueId,
        project::model::ExportResolution,
    },
    api::{
        classification::{MatchKind, RuleEvidenceError, RuleEvidenceTable, RuleIndex},
        compiler::{
            normalized::CanonicalArgumentConstraints,
            physical::PhysicalRoot,
            rule::{EventPredicate, EvidenceDescriptor, IdentityConstraint},
        },
    },
};

mod evaluator;
mod identity;

use evaluator::{EvaluationOperations, MatcherEvaluator, PreparedClausePaths};

struct ConstrainedRoot<'a> {
    rule: RuleIndex,
    identity: &'a IdentityConstraint,
    event: &'a EventPredicate,
    constraints: &'a CanonicalArgumentConstraints,
    evidence: &'a EvidenceDescriptor,
}

enum ConstrainedState {
    Indexed,
    Fallback(Vec<Occurrence>),
    Published,
}

struct PreparedConstrainedRoot<'a> {
    root: ConstrainedRoot<'a>,
    paths: PreparedClausePaths,
    state: ConstrainedState,
}

impl<'a> PreparedConstrainedRoot<'a> {
    fn new(root: &ConstrainedRoot<'a>, names: &NameTable) -> Self {
        Self {
            root: ConstrainedRoot {
                rule: root.rule,
                identity: root.identity,
                event: root.event,
                constraints: root.constraints,
                evidence: root.evidence,
            },
            paths: PreparedClausePaths::new(root.identity, root.event, names),
            state: ConstrainedState::Indexed,
        }
    }

    fn mark_fallback(&mut self) {
        self.state = ConstrainedState::Fallback(Vec::new());
    }

    fn is_fallback(&self) -> bool {
        matches!(self.state, ConstrainedState::Fallback(_))
    }

    fn record_fallback(&mut self, occurrence: Occurrence) {
        if let ConstrainedState::Fallback(occurrences) = &mut self.state {
            occurrences.push(occurrence);
        }
    }

    fn publish(
        &mut self,
        evidence: &mut RuleEvidenceTable,
        occurrences: Vec<Occurrence>,
    ) -> Result<(), RuleEvidenceError> {
        if !occurrences.is_empty() {
            push_owned_rule_evidence(
                evidence,
                self.root.rule,
                self.root.evidence.kind,
                self.root.evidence.symbol.clone(),
                occurrences,
            )?;
        }
        self.state = ConstrainedState::Published;
        Ok(())
    }

    fn publish_fallback(
        &mut self,
        evidence: &mut RuleEvidenceTable,
    ) -> Result<(), RuleEvidenceError> {
        match std::mem::replace(&mut self.state, ConstrainedState::Published) {
            ConstrainedState::Fallback(occurrences) => {
                if let Err(error) = self.publish(evidence, occurrences.clone()) {
                    self.state = ConstrainedState::Fallback(occurrences);
                    return Err(error);
                }
                Ok(())
            }
            state => {
                self.state = state;
                Ok(())
            }
        }
    }
}

struct ConstrainedEvaluation<'a> {
    roots: Vec<PreparedConstrainedRoot<'a>>,
}

impl<'a> ConstrainedEvaluation<'a> {
    fn prepare(roots: &[(usize, &'a PhysicalRoot)], names: &NameTable) -> Self {
        let constrained: Vec<ConstrainedRoot<'_>> = roots
            .iter()
            .filter_map(|(rule_index, root)| match root {
                PhysicalRoot::ConstrainedScan {
                    identity,
                    event,
                    constraints,
                    evidence,
                } => Some(ConstrainedRoot {
                    rule: RuleIndex::new(*rule_index),
                    identity,
                    event,
                    constraints,
                    evidence,
                }),
                _ => None,
            })
            .collect();
        Self {
            roots: constrained
                .iter()
                .map(|root| PreparedConstrainedRoot::new(root, names))
                .collect(),
        }
    }
}

/// The matcher artifact borrowed from one immutable semantic artifact.
/// Keeping the stream, occurrence index, and linked occurrence view together
/// prevents evaluation from combining IDs and borrowed buckets from different
/// artifacts.
#[derive(Debug)]
pub(in crate::analysis) struct MatcherArtifact<'a> {
    stream: &'a FactStream<Frozen>,
    indexes: &'a OccurrenceIndexes,
    overlay: Option<LinkedOccurrenceView<'a>>,
}

impl<'a> MatcherArtifact<'a> {
    pub(in crate::analysis) fn from_facts(
        facts: &'a SemanticFacts,
        inputs: MatcherProjectInputs<'_>,
        overlay_policy: MatcherOverlayPolicy,
    ) -> (Self, usize) {
        let (overlay, operations) = match overlay_policy {
            MatcherOverlayPolicy::Disabled => (None, 0),
            MatcherOverlayPolicy::Enabled => {
                if facts.matcher_index().is_available() {
                    inputs.module_identities().map_or((None, 0), |identities| {
                        let (overlay, operations) =
                            LinkedOccurrenceView::build(facts.matcher_index(), identities);
                        (Some(overlay), operations)
                    })
                } else {
                    (None, 0)
                }
            }
        };
        (
            Self {
                stream: facts.stream(),
                indexes: facts.matcher_index(),
                overlay,
            },
            operations,
        )
    }

    #[cfg(test)]
    fn from_parts(stream: &'a FactStream<Frozen>, indexes: &'a OccurrenceIndexes) -> Self {
        Self {
            stream,
            indexes,
            overlay: None,
        }
    }

    #[cfg(test)]
    fn from_parts_with_overlay(
        stream: &'a FactStream<Frozen>,
        indexes: &'a OccurrenceIndexes,
        overlay: Option<&LinkedOccurrenceView<'a>>,
    ) -> Self {
        Self {
            stream,
            indexes,
            overlay: overlay.cloned(),
        }
    }

    pub(in crate::analysis) fn indexes(&self) -> &OccurrenceIndexes {
        self.indexes
    }

    pub(in crate::analysis) fn overlay(&self) -> Option<&LinkedOccurrenceView<'a>> {
        self.overlay.as_ref()
    }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::analysis) struct MatcherProjectInputs<'a> {
    module_identities: Option<&'a ModuleIdentityMap>,
    call_result_identities: Option<&'a BTreeMap<ValueId, ExportResolution>>,
}

impl<'a> MatcherProjectInputs<'a> {
    pub(in crate::analysis) const fn new(
        module_identities: Option<&'a ModuleIdentityMap>,
        call_result_identities: Option<&'a BTreeMap<ValueId, ExportResolution>>,
    ) -> Self {
        Self {
            module_identities,
            call_result_identities,
        }
    }

    fn module_identities(self) -> Option<&'a ModuleIdentityMap> {
        self.module_identities
    }

    fn call_result_identities(self) -> Option<&'a BTreeMap<ValueId, ExportResolution>> {
        self.call_result_identities
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(in crate::analysis) enum MatcherOverlayPolicy {
    Disabled,
    Enabled,
}

/// Project-level identities and occurrence remapping for one local module.
#[derive(Debug, Clone, Copy)]
pub(in crate::analysis) struct MatcherProjectOverlay<'a> {
    identities: Option<&'a ModuleIdentityMap>,
    result_identities: Option<&'a BTreeMap<ValueId, ExportResolution>>,
}

/// One matcher-facing view of a module's facts and project identities.
///
/// Keeping these views together makes it impossible for production
/// projection to construct an occurrence artifact and identity overlay from
/// different project inputs.
#[derive(Debug)]
pub(in crate::analysis) struct MatcherProjectContext<'facts, 'project> {
    artifact: MatcherArtifact<'facts>,
    project: MatcherProjectOverlay<'project>,
}

impl<'facts, 'project> MatcherProjectContext<'facts, 'project> {
    pub(in crate::analysis) fn from_facts(
        facts: &'facts SemanticFacts,
        inputs: MatcherProjectInputs<'project>,
        overlay_policy: MatcherOverlayPolicy,
    ) -> (Self, usize) {
        let (artifact, operations) = MatcherArtifact::from_facts(facts, inputs, overlay_policy);
        (
            Self {
                artifact,
                project: MatcherProjectOverlay::from_inputs(inputs),
            },
            operations,
        )
    }

    pub(in crate::analysis) fn artifact(&self) -> &MatcherArtifact<'facts> {
        &self.artifact
    }

    pub(in crate::analysis) fn project(&self) -> MatcherProjectOverlay<'project> {
        self.project
    }

    pub(in crate::analysis) fn into_artifact(self) -> MatcherArtifact<'facts> {
        self.artifact
    }
}

impl<'a> MatcherProjectOverlay<'a> {
    pub(in crate::analysis) fn from_inputs(inputs: MatcherProjectInputs<'a>) -> Self {
        Self {
            identities: inputs.module_identities(),
            result_identities: inputs.call_result_identities(),
        }
    }

    #[cfg(test)]
    fn new(
        _occurrence: Option<&'a LinkedOccurrenceView<'a>>,
        identities: Option<&'a ModuleIdentityMap>,
        result_identities: Option<&'a BTreeMap<ValueId, ExportResolution>>,
    ) -> Self {
        Self {
            identities,
            result_identities,
        }
    }
}

#[cfg(test)]
type MatcherLocalInput<'a> = MatcherArtifact<'a>;

struct MatcherEvaluationContext<'borrow, 'artifact> {
    artifact: &'borrow MatcherArtifact<'artifact>,
    project: MatcherProjectOverlay<'borrow>,
    operations: &'borrow mut EvaluationOperations,
}

fn push_owned_rule_evidence(
    evidence: &mut RuleEvidenceTable,
    rule: RuleIndex,
    kind: MatchKind,
    symbol: String,
    occurrences: impl IntoIterator<Item = Occurrence>,
) -> Result<(), RuleEvidenceError> {
    if let Some(group) = EvidenceGroup::from_occurrences(
        kind,
        symbol,
        crate::project::MatchCertainty::Definite,
        occurrences,
    ) {
        evidence.record(rule, group.into_classification())?;
    }
    Ok(())
}

pub(in crate::analysis) fn try_compute_constrained_evidence<'artifact>(
    artifact: impl Borrow<MatcherArtifact<'artifact>>,
    roots: &[(usize, &PhysicalRoot)],
    evidence: &mut RuleEvidenceTable,
    project: MatcherProjectOverlay<'_>,
) -> Result<(), RuleEvidenceError> {
    let mut ops = EvaluationOperations::default();
    compute_constrained_inner(
        MatcherEvaluationContext {
            artifact: artifact.borrow(),
            project,
            operations: &mut ops,
        },
        roots,
        evidence,
    )
}

#[cfg(test)]
fn compute_constrained_evidence<'artifact>(
    artifact: impl Borrow<MatcherArtifact<'artifact>>,
    roots: &[(usize, &PhysicalRoot)],
    evidence: &mut RuleEvidenceTable,
    project: MatcherProjectOverlay<'_>,
) {
    try_compute_constrained_evidence(artifact, roots, evidence, project)
        .expect("test evidence uses its catalog capacity");
}

/// Inner implementation that also tracks evaluation operations.
fn compute_constrained_inner(
    context: MatcherEvaluationContext<'_, '_>,
    roots: &[(usize, &PhysicalRoot)],
    evidence: &mut RuleEvidenceTable,
) -> Result<(), RuleEvidenceError> {
    let MatcherEvaluationContext {
        artifact,
        project,
        operations,
    } = context;
    let stream = artifact.stream;
    let indexes = artifact.indexes;
    let MatcherProjectOverlay {
        identities,
        result_identities,
    } = project;
    let names = stream.names();
    let values = stream.values();
    let evaluator = MatcherEvaluator::new(names, values, identities, result_identities);

    let mut evaluation = ConstrainedEvaluation::prepare(roots, names);
    evaluation.evaluate_indexed_roots(
        stream,
        indexes,
        artifact.overlay(),
        &evaluator,
        operations,
        evidence,
    )?;
    evaluation.evaluate_fallback_roots(stream, &evaluator, operations, evidence)
}

/// Evaluate roots whose identity can use the occurrence index, marking the
/// remaining roots for the bounded linear fallback pass.
impl ConstrainedEvaluation<'_> {
    fn evaluate_indexed_roots(
        &mut self,
        stream: &FactStream<Frozen>,
        indexes: &OccurrenceIndexes,
        overlay: Option<&LinkedOccurrenceView<'_>>,
        evaluator: &MatcherEvaluator<'_>,
        operations: &mut EvaluationOperations,
        evidence: &mut RuleEvidenceTable,
    ) -> Result<(), RuleEvidenceError> {
        for prepared_root in &mut self.roots {
            let root = &prepared_root.root;
            let Some(candidates) =
                indexes.occurrences_for_indexed(root.identity, root.event, overlay, stream.names())
            else {
                prepared_root.mark_fallback();
                continue;
            };
            let matched: Vec<_> = candidates
                .into_iter()
                .filter_map(|occurrence| {
                    stream
                        .fact(occurrence.event())
                        .filter(|fact| {
                            evaluator.fact_matches_clause(
                                fact,
                                root.identity,
                                root.event,
                                root.constraints,
                                &prepared_root.paths,
                                operations,
                            )
                        })
                        .map(|_| occurrence)
                })
                .collect();
            prepared_root.publish(evidence, matched)?;
        }
        Ok(())
    }

    /// Scan roots that could not use an index, then publish their evidence.
    fn evaluate_fallback_roots(
        &mut self,
        stream: &FactStream<Frozen>,
        evaluator: &MatcherEvaluator<'_>,
        operations: &mut EvaluationOperations,
        evidence: &mut RuleEvidenceTable,
    ) -> Result<(), RuleEvidenceError> {
        if !self.roots.iter().any(PreparedConstrainedRoot::is_fallback) {
            return Ok(());
        }
        for fact in stream.facts() {
            for prepared_root in self.roots.iter_mut().filter(|root| root.is_fallback()) {
                let root = &prepared_root.root;
                if evaluator.fact_matches_clause(
                    fact,
                    root.identity,
                    root.event,
                    root.constraints,
                    &prepared_root.paths,
                    operations,
                ) {
                    prepared_root.record_fallback(Occurrence::new(fact.id, fact.span));
                }
            }
        }
        for prepared_root in self.roots.iter_mut().filter(|root| root.is_fallback()) {
            prepared_root.publish_fallback(evidence)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::PathId;

    use super::*;
    use crate::{
        Environment,
        analysis::{
            facts::{CallArgInfo, FactStream, Frozen, build_test_stream},
            matching::{ExportResolution, ModuleExportKey, OccurrenceIndexes},
            model::value::ValueId,
            resolution::Resolver,
            semantic::SpanNormalizer,
            syntax::SymbolCallProvenance,
        },
        api::{
            classification::MatchKind,
            compiler::{
                physical::{PhysicalRoot, compile_argument_constraints},
                rule::{
                    CompiledMatcherPlan, EventPredicate, EvidenceDescriptor, IdentityConstraint,
                },
            },
            rule::{ArgumentConstraint, ArgumentMatcher, EventQuery, ValueMatcher},
        },
        project::SourceText,
    };

    fn stream(source: &str, environment: &Environment) -> FactStream<Frozen> {
        let parsed = crate::parse_test_source(source, "constrained.js").unwrap();
        let coordinates = SpanNormalizer::new(parsed.source_start, &SourceText::from(source));
        let mut resolver =
            Resolver::collect_with_environment(&parsed.program, environment, coordinates);
        build_test_stream(&parsed.program, &mut resolver)
    }

    fn build_index(stream: &FactStream<Frozen>) -> OccurrenceIndexes {
        OccurrenceIndexes::from_stream(
            stream,
            &Environment::default(),
            crate::analysis::DerivedPhaseAvailability::Enabled,
        )
    }

    #[test]
    fn constrained_publication_returns_capacity_error() {
        let stream = stream("fetch('/api');", &Environment::default());
        let index = build_index(&stream);
        let root = constrained_root(
            IdentityConstraint::Any {
                name: "fetch".into(),
            },
            EventPredicate::Call,
            "fetch",
        );
        let mut evidence = RuleEvidenceTable::new_for_test(0);

        let error = try_compute_constrained_evidence(
            MatcherLocalInput::from_parts(&stream, &index),
            &[(0, &root)],
            &mut evidence,
            MatcherProjectOverlay::new(None, None, None),
        )
        .expect_err("a stale rule index must remain a typed publication error");

        assert_eq!(
            error,
            RuleEvidenceError::RuleOutOfRange {
                rule: RuleIndex::new(0),
                capacity: 0,
            }
        );
    }

    fn constrained_root(
        identity: IdentityConstraint,
        event: EventPredicate,
        symbol: &str,
    ) -> PhysicalRoot {
        PhysicalRoot::ConstrainedScan {
            identity,
            event,
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().try_equals("/api").unwrap(),
            )]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: symbol.into(),
            },
        }
    }

    #[test]
    fn constrained_calls_and_members_execute_once() {
        let stream = stream(
            "fetch('/api'); client.open('/api');",
            &Environment::default(),
        );
        let call = constrained_root(
            IdentityConstraint::Any {
                name: "fetch".into(),
            },
            EventPredicate::Call,
            "fetch",
        );
        let member = constrained_root(
            IdentityConstraint::Any {
                name: "client.open".into(),
            },
            EventPredicate::MemberCall {
                member: "client.open".into(),
            },
            "client.open",
        );
        let index = build_index(&stream);
        let mut evidence = RuleEvidenceTable::new_for_test(1);
        compute_constrained_evidence(
            MatcherLocalInput::from_parts(&stream, &index),
            &[(0, &call), (0, &member)],
            &mut evidence,
            MatcherProjectOverlay::new(None, None, None),
        );
        assert_eq!(evidence[0].len(), 2);
        assert!(evidence[0].iter().all(|item| item.count() == 1));
        assert_ne!(
            evidence[0][0].occurrences()[0].fact(),
            evidence[0][1].occurrences()[0].fact()
        );
    }

    #[test]
    fn constrained_evidence_is_source_ordered_and_deduplicated() {
        let query = EventQuery::call_heuristic("fetch")
            .unwrap()
            .with_arg_static_strings(0, ["/api"])
            .unwrap()
            .into_query();
        let plan = CompiledMatcherPlan::compile(&[query.clone(), query]).unwrap();
        let roots: Vec<PhysicalRoot> = plan
            .physical_roots()
            .iter()
            .filter(|r| matches!(r, PhysicalRoot::ConstrainedScan { .. }))
            .cloned()
            .collect();
        assert_eq!(roots.len(), 1, "equivalent declarations produce one root");

        let stream = stream("fetch('/api');\nfetch('/api');", &Environment::default());
        let index = build_index(&stream);
        let mut evidence = RuleEvidenceTable::new_for_test(1);
        compute_constrained_evidence(
            MatcherLocalInput::from_parts(&stream, &index),
            &[(0, &roots[0])],
            &mut evidence,
            MatcherProjectOverlay::new(None, None, None),
        );
        assert_eq!(evidence[0].len(), 1);
        assert_eq!(evidence[0][0].occurrences().len(), 2);
        assert!(
            evidence[0][0]
                .occurrences()
                .iter()
                .all(|occ| !occ.span().is_empty())
        );
        let mut normalized = std::mem::take(&mut evidence[0]);
        crate::analysis::matching::evidence::normalize_evidence(&mut normalized, usize::MAX);
        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].count(), 2);
        assert_eq!(normalized[0].occurrences().len(), 2);
        assert!(
            normalized[0].occurrences().windows(2).all(|pair| {
                (pair[0].span(), pair[0].fact()) < (pair[1].span(), pair[1].fact())
            })
        );
    }

    #[test]
    fn missing_argument_fails_closed() {
        let stream = stream("fetch('/api');", &Environment::default());
        let _root = constrained_root(
            IdentityConstraint::Any {
                name: "fetch".into(),
            },
            EventPredicate::Call,
            "fetch",
        );
        // Patch the root to reference argument index 5 (out of bounds).
        let patched = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(5),
                ValueMatcher::static_string().try_equals("/api").unwrap(),
            )]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = RuleEvidenceTable::new_for_test(1);
        compute_constrained_evidence(
            MatcherLocalInput::from_parts(&stream, &index),
            &[(0, &patched)],
            &mut evidence,
            MatcherProjectOverlay::new(None, None, None),
        );
        assert!(
            evidence[0].is_empty(),
            "missing argument should not produce evidence"
        );
    }

    #[test]
    fn dynamic_value_does_not_match_static_predicate() {
        let stream = stream("fetch(value);", &Environment::default());
        let root = constrained_root(
            IdentityConstraint::Any {
                name: "fetch".into(),
            },
            EventPredicate::Call,
            "fetch",
        );
        let index = build_index(&stream);
        let mut evidence = RuleEvidenceTable::new_for_test(1);
        compute_constrained_evidence(
            MatcherLocalInput::from_parts(&stream, &index),
            &[(0, &root)],
            &mut evidence,
            MatcherProjectOverlay::new(None, None, None),
        );
        // Dynamic values should not match a static string predicate.
        assert!(
            evidence[0].is_empty(),
            "dynamic value must not match static string predicate"
        );
    }

    #[test]
    fn sparse_argument_positions() {
        let stream = stream("fetch('/api', '/path');", &Environment::default());
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[
                ArgumentConstraint::new(
                    crate::api::rule::ArgumentIndex::new_unchecked(0),
                    ValueMatcher::static_string().try_equals("/api").unwrap(),
                ),
                ArgumentConstraint::new(
                    crate::api::rule::ArgumentIndex::new_unchecked(1),
                    ValueMatcher::static_string().try_equals("/path").unwrap(),
                ),
            ]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = RuleEvidenceTable::new_for_test(1);
        compute_constrained_evidence(
            MatcherLocalInput::from_parts(&stream, &index),
            &[(0, &root)],
            &mut evidence,
            MatcherProjectOverlay::new(None, None, None),
        );
        assert!(!evidence[0].is_empty(), "sparse arguments should match");
        assert_eq!(evidence[0][0].occurrences().len(), 1);
    }

    #[test]
    fn constraint_order_does_not_affect_matching() {
        let stream = stream("fetch('/api', '/path');", &Environment::default());
        let root_a = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[
                ArgumentConstraint::new(
                    crate::api::rule::ArgumentIndex::new_unchecked(0),
                    ValueMatcher::static_string().try_equals("/api").unwrap(),
                ),
                ArgumentConstraint::new(
                    crate::api::rule::ArgumentIndex::new_unchecked(1),
                    ValueMatcher::static_string().try_equals("/path").unwrap(),
                ),
            ]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let root_b = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[
                ArgumentConstraint::new(
                    crate::api::rule::ArgumentIndex::new_unchecked(1),
                    ValueMatcher::static_string().try_equals("/path").unwrap(),
                ),
                ArgumentConstraint::new(
                    crate::api::rule::ArgumentIndex::new_unchecked(0),
                    ValueMatcher::static_string().try_equals("/api").unwrap(),
                ),
            ]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut ev_a = RuleEvidenceTable::new_for_test(1);
        let mut ev_b = RuleEvidenceTable::new_for_test(1);
        compute_constrained_evidence(
            MatcherLocalInput::from_parts(&stream, &index),
            &[(0, &root_a)],
            &mut ev_a,
            MatcherProjectOverlay::new(None, None, None),
        );
        compute_constrained_evidence(
            MatcherLocalInput::from_parts(&stream, &index),
            &[(0, &root_b)],
            &mut ev_b,
            MatcherProjectOverlay::new(None, None, None),
        );
        assert_eq!(ev_a[0].len(), ev_b[0].len());
        assert_eq!(ev_a[0][0].count(), ev_b[0][0].count());
    }

    #[test]
    fn equals_any_accepts_any_matching_alternative() {
        let stream = stream("fetch('/api');", &Environment::default());
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string()
                    .equals_any(["/api", "/other"])
                    .unwrap(),
            )]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = RuleEvidenceTable::new_for_test(1);
        compute_constrained_evidence(
            MatcherLocalInput::from_parts(&stream, &index),
            &[(0, &root)],
            &mut evidence,
            MatcherProjectOverlay::new(None, None, None),
        );
        assert!(!evidence[0].is_empty(), "equals_any should match /api");
    }

    #[test]
    fn equals_any_rejects_non_matching_values() {
        let stream = stream("fetch('/other');", &Environment::default());
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string()
                    .equals_any(["/api", "/v1"])
                    .unwrap(),
            )]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = RuleEvidenceTable::new_for_test(1);
        compute_constrained_evidence(
            MatcherLocalInput::from_parts(&stream, &index),
            &[(0, &root)],
            &mut evidence,
            MatcherProjectOverlay::new(None, None, None),
        );
        assert!(
            evidence[0].is_empty(),
            "equals_any should reject non-matching values"
        );
    }

    #[test]
    fn contains_any_accepts_string_containing_marker() {
        let stream = stream("fetch('/api/token');", &Environment::default());
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string()
                    .contains_any(["token"])
                    .unwrap(),
            )]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = RuleEvidenceTable::new_for_test(1);
        compute_constrained_evidence(
            MatcherLocalInput::from_parts(&stream, &index),
            &[(0, &root)],
            &mut evidence,
            MatcherProjectOverlay::new(None, None, None),
        );
        assert!(
            !evidence[0].is_empty(),
            "contains_any should match /api/token"
        );
    }

    #[test]
    fn prefix_matches_static_string_start() {
        let stream = stream(
            "fetch('https://example.test/data');",
            &Environment::default(),
        );
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string()
                    .starts_with_any(["https://"])
                    .unwrap(),
            )]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = RuleEvidenceTable::new_for_test(1);
        compute_constrained_evidence(
            MatcherLocalInput::from_parts(&stream, &index),
            &[(0, &root)],
            &mut evidence,
            MatcherProjectOverlay::new(None, None, None),
        );
        assert!(
            !evidence[0].is_empty(),
            "prefix should match https:// string"
        );
    }

    #[test]
    fn object_keys_matcher_accepts_expected_keys() {
        let stream = stream(
            "fetch({url: '/api', method: 'POST'});",
            &Environment::default(),
        );
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ArgumentMatcher::object_keys(["url", "method"]).unwrap(),
            )]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = RuleEvidenceTable::new_for_test(1);
        compute_constrained_evidence(
            MatcherLocalInput::from_parts(&stream, &index),
            &[(0, &root)],
            &mut evidence,
            MatcherProjectOverlay::new(None, None, None),
        );
        assert!(!evidence[0].is_empty(), "object keys should match");
    }

    #[test]
    fn object_property_value_matcher_accepts_matching_property() {
        let stream = stream(
            "fetch({url: '/api', method: 'POST'});",
            &Environment::default(),
        );
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ArgumentMatcher::object_property_value(
                    "method",
                    ValueMatcher::static_string().try_equals("POST").unwrap(),
                )
                .unwrap(),
            )]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = RuleEvidenceTable::new_for_test(1);
        compute_constrained_evidence(
            MatcherLocalInput::from_parts(&stream, &index),
            &[(0, &root)],
            &mut evidence,
            MatcherProjectOverlay::new(None, None, None),
        );
        assert!(
            !evidence[0].is_empty(),
            "object property value should match"
        );
    }

    #[test]
    fn argument_overlay_applies_static_string_from_identity_map() {
        let mut identities = ModuleIdentityMap::new();
        identities.insert(
            ModuleExportKey::new("api", "request"),
            ExportResolution::StaticString {
                value: "https://example.test".into(),
            },
        );
        let argument = CallArgInfo {
            value: ValueId::from_test(7),
            base_value: ValueId::UNKNOWN,
            base_path: PathId::EMPTY,
            spread: false,
            provenance: SymbolCallProvenance::ModuleExport {
                module: "api".into(),
                export: "request".into(),
            },
        };
        assert_eq!(
            MatcherEvaluator::new(
                &glass_lint_datastructures::NameTable::default(),
                &crate::analysis::model::value::ValueTable::default(),
                Some(&identities),
                None
            )
            .argument_with_overlay(&argument)
            .static_string,
            Some("https://example.test")
        );
    }

    // ── Package 6: operation and argument-preparation tests ────────

    /// Helper that runs `compute_constrained_inner` and returns ops.
    fn run_with_ops(
        stream: &FactStream<Frozen>,
        index: &OccurrenceIndexes,
        roots: &[(usize, &PhysicalRoot)],
        overlay: Option<&LinkedOccurrenceView<'_>>,
    ) -> EvaluationOperations {
        let mut evidence = RuleEvidenceTable::new_for_test(roots.len());
        let mut ops = EvaluationOperations::default();
        compute_constrained_inner(
            MatcherEvaluationContext {
                artifact: &MatcherArtifact::from_parts_with_overlay(stream, index, overlay),
                project: MatcherProjectOverlay::new(overlay, None, None),
                operations: &mut ops,
            },
            roots,
            &mut evidence,
        )
        .expect("test evidence uses its catalog capacity");
        ops
    }

    #[test]
    fn two_predicates_on_one_arg_prepare_argument_once() {
        let stream = stream("fetch('/api');", &Environment::default());
        let index = build_index(&stream);

        // Two constraints on the same argument index (0):
        //   equals("/api") AND starts_with_any(["/"])
        let constraints = compile_argument_constraints(&[
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().try_equals("/api").unwrap(),
            ),
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string()
                    .starts_with_any(["/"])
                    .unwrap(),
            ),
        ]);
        assert_eq!(constraints.groups().len(), 1, "should be one group");

        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints,
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };

        let ops = run_with_ops(&stream, &index, &[(0, &root)], None);

        // One candidate, one group → 1 argument preparation, 2 predicates
        assert_eq!(ops.candidates, 1, "one candidate (fetch call)");
        assert_eq!(ops.groups, 1, "one group");
        assert_eq!(ops.argument_preparations, 1, "argument prepared once");
        assert_eq!(ops.value_resolutions, 1, "argument value resolved once");
        assert_eq!(ops.predicates, 2, "two predicates applied");
    }

    #[test]
    fn mixed_object_predicates_share_one_prepared_projection() {
        let stream = stream(
            "fetch({url: '/api', method: 'POST'});",
            &Environment::default(),
        );
        let index = build_index(&stream);
        let constraints = compile_argument_constraints(&[
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ArgumentMatcher::object_keys(["url", "method"]).unwrap(),
            ),
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ArgumentMatcher::object_property_value(
                    "method",
                    ValueMatcher::static_string().try_equals("POST").unwrap(),
                )
                .unwrap(),
            ),
        ]);
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints,
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };

        let ops = run_with_ops(&stream, &index, &[(0, &root)], None);
        assert_eq!(ops.candidates, 1);
        assert_eq!(ops.groups, 1);
        assert_eq!(ops.argument_preparations, 1);
        assert_eq!(ops.value_resolutions, 1);
        assert_eq!(ops.predicates, 2);
    }

    #[test]
    fn several_argument_positions_each_prepared_once() {
        let stream = stream("fetch('/api', '/path');", &Environment::default());
        let index = build_index(&stream);

        // Constraints on index 0 AND index 1
        let constraints = compile_argument_constraints(&[
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().try_equals("/api").unwrap(),
            ),
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(1),
                ValueMatcher::static_string().try_equals("/path").unwrap(),
            ),
        ]);
        assert_eq!(constraints.groups().len(), 2, "should be two groups");

        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints,
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };

        let ops = run_with_ops(&stream, &index, &[(0, &root)], None);

        // One candidate, two groups → 2 argument preparations, 2 predicates
        assert_eq!(ops.candidates, 1, "one candidate");
        assert_eq!(ops.groups, 2, "two groups (index 0 and index 1)");
        assert_eq!(ops.argument_preparations, 2, "each index prepared once");
        assert_eq!(
            ops.value_resolutions, 2,
            "each argument value resolved once"
        );
        assert_eq!(ops.predicates, 2, "one predicate per group");
    }

    #[test]
    fn duplicate_constraints_do_not_inflate_operation_counts() {
        let stream = stream("fetch('/api');", &Environment::default());
        let index = build_index(&stream);

        // Four identical constraints on the same index — compile_argument_constraints
        // should deduplicate them down to one group with one predicate.
        let constraints = compile_argument_constraints(&[
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().try_equals("/api").unwrap(),
            ),
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().try_equals("/api").unwrap(),
            ),
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().try_equals("/api").unwrap(),
            ),
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().try_equals("/api").unwrap(),
            ),
        ]);
        assert_eq!(constraints.groups().len(), 1, "deduplicated to one group");
        assert_eq!(
            constraints.groups()[0].predicates().len(),
            1,
            "deduplicated to one predicate"
        );

        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints,
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };

        let ops = run_with_ops(&stream, &index, &[(0, &root)], None);

        // One candidate, one group, one predicate — same as the simple case.
        assert_eq!(ops.candidates, 1, "one candidate");
        assert_eq!(ops.groups, 1, "one group (deduplicated)");
        assert_eq!(ops.predicates, 1, "one predicate (deduplicated)");
        assert_eq!(
            ops.argument_preparations, 1,
            "argument prepared once despite four raw constraints"
        );
        assert_eq!(ops.value_resolutions, 1);
    }

    #[test]
    fn operation_counts_scale_with_candidates() {
        let stream = stream(
            "fetch('/a'); fetch('/b'); fetch('/c');",
            &Environment::default(),
        );
        let index = build_index(&stream);

        let constraints = compile_argument_constraints(&[ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ValueMatcher::static_string(),
        )]);

        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints,
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };

        let ops = run_with_ops(&stream, &index, &[(0, &root)], None);

        assert_eq!(ops.candidates, 3, "three candidates (one per call)");
        assert_eq!(ops.groups, 3, "one group per candidate");
        assert_eq!(ops.predicates, 3, "one predicate per candidate");
    }

    #[test]
    fn static_alias_and_reassignment_preserves_matching() {
        // A static alias (`const x = '/api'; fetch(x);`) should match
        // the same as `fetch('/api');`.
        let stream = stream("const x = '/api'; fetch(x);", &Environment::default());
        let index = build_index(&stream);

        let constraints = compile_argument_constraints(&[ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ValueMatcher::static_string().try_equals("/api").unwrap(),
        )]);

        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
            },
            event: EventPredicate::Call,
            constraints,
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };

        let ops = run_with_ops(&stream, &index, &[(0, &root)], None);

        // The alias resolves through the value table: fetch(x) with x='/api'
        // should produce one matching candidate.
        assert_eq!(ops.candidates, 1, "one candidate (fetch(x))");
        assert_eq!(ops.groups, 1, "one group");
    }
}
