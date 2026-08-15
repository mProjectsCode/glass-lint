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
            rule::{EventSpec, EvidenceDescriptor, IdentityConstraint},
        },
    },
};

mod evaluator;
mod identity;

use evaluator::{EvaluationOperations, MatcherEvaluator, PreparedClausePaths};

struct ConstrainedRoot<'a> {
    rule: RuleIndex,
    identity: &'a IdentityConstraint,
    event: &'a EventSpec,
    constraints: &'a CanonicalArgumentConstraints,
    evidence: &'a EvidenceDescriptor,
}

pub(in crate::analysis) struct ConstrainedRootInput<'a> {
    rule_index: RuleIndex,
    root: &'a PhysicalRoot,
}

impl<'a> ConstrainedRootInput<'a> {
    pub(in crate::analysis) fn new(rule_index: RuleIndex, root: &'a PhysicalRoot) -> Self {
        Self { rule_index, root }
    }
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
    fn from_input(input: &ConstrainedRootInput<'a>, names: &NameTable) -> Option<Self> {
        let PhysicalRoot::ConstrainedScan {
            identity,
            event,
            constraints,
            evidence,
        } = input.root
        else {
            return None;
        };
        Some(Self {
            root: ConstrainedRoot {
                rule: input.rule_index,
                identity,
                event,
                constraints,
                evidence,
            },
            paths: PreparedClausePaths::new(identity, event, names),
            state: ConstrainedState::Indexed,
        })
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
            }
            state => {
                self.state = state;
            }
        }
        Ok(())
    }
}

struct ConstrainedEvaluation<'a> {
    roots: Vec<PreparedConstrainedRoot<'a>>,
}

impl<'a> ConstrainedEvaluation<'a> {
    fn prepare(roots: &[ConstrainedRootInput<'a>], names: &NameTable) -> Self {
        let roots = roots
            .iter()
            .filter_map(|input| PreparedConstrainedRoot::from_input(input, names))
            .collect();
        Self { roots }
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
        project: MatcherProjectOverlay<'_>,
        overlay_policy: MatcherOverlayPolicy,
    ) -> (Self, usize) {
        let (overlay, operations) = match overlay_policy {
            MatcherOverlayPolicy::Disabled => (None, 0),
            MatcherOverlayPolicy::Enabled => {
                if facts.matcher_index().is_available() {
                    project.identities.map_or((None, 0), |identities| {
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
        project: MatcherProjectOverlay<'project>,
        overlay_policy: MatcherOverlayPolicy,
    ) -> (Self, usize) {
        let (artifact, operations) = MatcherArtifact::from_facts(facts, project, overlay_policy);
        (Self { artifact, project }, operations)
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
    pub(in crate::analysis) const fn new(
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
    roots: &[ConstrainedRootInput<'_>],
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
    roots: &[ConstrainedRootInput<'_>],
    evidence: &mut RuleEvidenceTable,
    project: MatcherProjectOverlay<'_>,
) {
    try_compute_constrained_evidence(artifact, roots, evidence, project)
        .expect("test evidence uses its catalog capacity");
}

/// Inner implementation that also tracks evaluation operations.
fn compute_constrained_inner(
    context: MatcherEvaluationContext<'_, '_>,
    roots: &[ConstrainedRootInput<'_>],
    evidence: &mut RuleEvidenceTable,
) -> Result<(), RuleEvidenceError> {
    let MatcherEvaluationContext {
        artifact,
        project,
        operations,
    } = context;
    let stream = artifact.stream;
    let indexes = artifact.indexes;
    let names = stream.names();
    let values = stream.values();
    let evaluator = MatcherEvaluator::new(names, values, project);

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
mod tests;
