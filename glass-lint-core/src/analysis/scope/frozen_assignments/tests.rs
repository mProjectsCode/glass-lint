use glass_lint_datastructures::NameTable;
use swc_common::{BytePos, Span};

use super::*;
use crate::analysis::model::scope::{ProvenanceAlternatives, ProvenanceJoin};

#[test]
fn resolution_status_keeps_complete_witnesses_with_incomplete_joins() {
    let mut names = NameTable::default();
    let name = names.intern("value").unwrap();
    let scope = ScopeId::from_test(1);
    let span = Span::new(BytePos(0), BytePos(1));
    let alias = BindingProvenance::ValueAlias {
        target: glass_lint_datastructures::NamePath::new(),
    };
    let mut join = ProvenanceJoin::new(2);
    join.add(&ProvenanceAlternatives::single(alias.clone()));
    join.add(&ProvenanceAlternatives::unknown());
    let assignment = AliasAssignment::from_alternatives(
        span,
        scope,
        name,
        BindingVersion::from_test(1),
        join.alternatives().clone(),
    );

    let resolution = BindingResolution::assignment(&assignment);
    assert_eq!(resolution.status(), BindingResolutionStatus::Incomplete);
    assert_eq!(resolution.preferred_witness(), Some(&alias));
    let mut witnesses = Vec::new();
    resolution.for_each_witness(|witness| witnesses.push(witness));
    assert_eq!(witnesses, vec![&alias]);
}

#[test]
fn absent_assignment_uses_a_complete_declaration_witness() {
    let declaration = BindingProvenance::Local;
    let resolution = AssignmentAt::Absent.resolve(None, &declaration);
    assert_eq!(resolution.status(), BindingResolutionStatus::Complete);
    assert_eq!(resolution.preferred_witness(), Some(&declaration));
}
