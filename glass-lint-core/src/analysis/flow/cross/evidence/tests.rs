use super::*;
use crate::{
    analysis::{
        facts::FactId,
        model::flow::{FlowId, RequirementIndex, SinkIndex},
        trace::QualifiedEvent,
    },
    api::classification::{ClassificationEvidenceOccurrence, RuleEvidenceCapacity, RuleIndex},
};

#[test]
fn trace_assembly_keeps_prior_sinks_as_sinks() {
    let mut state = CrossFlowState::known(
        FlowId::new(RuleIndex::new(0), 0),
        QualifiedEvent::new(ModuleId::new(1), FactId::new(1)),
    );
    state.record_requirement_for_test(
        RequirementIndex::new(0).unwrap(),
        QualifiedEvent::new(ModuleId::new(1), FactId::new(2)),
    );
    state.record_sink_for_test(
        SinkIndex::new(0).unwrap(),
        QualifiedEvent::new(ModuleId::new(1), FactId::new(3)),
    );
    let mut arena = TraceArena::new(10);
    let head = assemble_trace(&mut arena, &state, ModuleId::new(1), FactId::new(4))
        .expect("trace should fit within the arena");
    let roles: Vec<_> = arena
        .reconstruct_trace(head)
        .unwrap()
        .into_iter()
        .map(|step| step.role())
        .collect();
    assert_eq!(
        roles,
        vec![
            EvidenceRole::Source,
            EvidenceRole::Requirement,
            EvidenceRole::Sink,
            EvidenceRole::Sink,
        ]
    );
}

#[test]
fn incomplete_projection_keeps_cross_evidence_as_possible() {
    let rule = RuleIndex::new(0);
    let key = EvidenceKey {
        kind: MatchKind::CallArgument,
        symbol: "fetch".to_owned(),
        fact: FactId::from_test(1),
    };
    let mut evidence = ModuleEvidence::new(RuleEvidenceCapacity::from_catalog_len(1));
    evidence.record(
        rule,
        &key,
        ClassificationEvidence::from_occurrence(
            MatchKind::CallArgument,
            "fetch".to_owned(),
            ClassificationEvidenceOccurrence::new(
                glass_lint_datastructures::ByteRange::empty(),
                Some(1),
                None,
            ),
            crate::project::MatchCertainty::Definite,
        ),
    );

    evidence.mark_all_possible();

    let items = evidence.into_evidence();
    assert_eq!(
        items.for_rule(rule).unwrap()[0].certainty(),
        crate::project::MatchCertainty::Possible
    );
}
