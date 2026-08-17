#[cfg(test)]
mod test_indexing {
    use std::ops::{Index, IndexMut};

    use crate::api::classification::{ClassificationEvidence, RuleEvidenceTable, RuleIndex};

    static EMPTY: Vec<ClassificationEvidence> = Vec::new();

    impl Index<usize> for RuleEvidenceTable {
        type Output = Vec<ClassificationEvidence>;

        fn index(&self, index: usize) -> &Self::Output {
            self.values.get(&RuleIndex::new(index)).unwrap_or(&EMPTY)
        }
    }

    impl IndexMut<usize> for RuleEvidenceTable {
        fn index_mut(&mut self, index: usize) -> &mut Self::Output {
            self.items_mut(RuleIndex::new(index))
                .expect("test index is in range")
        }
    }
}

#[cfg(test)]
mod test_evidence_capacity {
    use glass_lint_datastructures::ByteRange;

    use crate::api::classification::{
        ClassificationEvidence, ClassificationEvidenceOccurrence, MatchCertainty, MatchKind,
        RuleEvidenceError, RuleEvidenceTable, RuleIndex,
    };

    fn evidence() -> ClassificationEvidence {
        ClassificationEvidence {
            kind: MatchKind::Call,
            symbol: "fetch".to_owned(),
            count: 1,
            truncated: false,
            certainty: MatchCertainty::Definite,
            occurrences: vec![ClassificationEvidenceOccurrence {
                span: ByteRange::empty(),
                fact: None,
                trace: None,
            }],
        }
    }

    #[test]
    fn rejects_rule_indices_outside_catalog_capacity() {
        let mut table = RuleEvidenceTable::new_for_test(1);

        assert_eq!(
            table.record(RuleIndex::new(1), evidence()),
            Err(RuleEvidenceError::RuleOutOfRange {
                rule: RuleIndex::new(1),
                capacity: 1,
            })
        );
    }

    #[test]
    fn rejects_merging_different_capacities_without_mutating_destination() {
        let mut destination = RuleEvidenceTable::new_for_test(1);
        let mut other = RuleEvidenceTable::new_for_test(2);
        other.record(RuleIndex::new(1), evidence()).unwrap();

        assert_eq!(
            destination.merge_equal_capacity(other),
            Err(RuleEvidenceError::CapacityMismatch {
                expected: 1,
                actual: 2,
            })
        );
        assert!(destination.for_rule(RuleIndex::new(1)).is_none());
    }

    #[test]
    fn evidence_constructors_preserve_count_and_occurrence_invariants() {
        let occurrence = ClassificationEvidenceOccurrence::new(ByteRange::empty(), Some(1), None);
        assert!(
            ClassificationEvidence::from_occurrences(
                MatchKind::Call,
                "fetch".into(),
                Vec::new(),
                MatchCertainty::Definite,
            )
            .is_none()
        );
        assert!(
            ClassificationEvidence::from_parts(
                MatchKind::Call,
                "fetch".into(),
                0,
                false,
                MatchCertainty::Definite,
                vec![occurrence],
            )
            .is_none()
        );

        let mut evidence = ClassificationEvidence::from_occurrences(
            MatchKind::Call,
            "fetch".into(),
            vec![occurrence],
            MatchCertainty::Definite,
        )
        .unwrap();
        evidence.mark_truncated();
        assert_eq!(evidence.count(), 1);
        assert!(evidence.is_truncated());

        let direct = ClassificationEvidence::from_occurrence(
            MatchKind::Call,
            "fetch".into(),
            occurrence,
            MatchCertainty::Possible,
        );
        assert_eq!(direct.count(), 1);
        assert_eq!(direct.occurrences(), &[occurrence]);
    }
}
