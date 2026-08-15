use glass_lint_datastructures::ByteRange;

use super::*;

fn evidence(symbol: &str, spans: &[u32]) -> ClassificationEvidence {
    ClassificationEvidence::from_occurrences(
        MatchKind::Call,
        symbol.into(),
        spans
            .iter()
            .map(|position| {
                crate::api::classification::ClassificationEvidenceOccurrence::new(
                    ByteRange::new(*position, *position + 1).unwrap(),
                    Some(*position),
                    None,
                )
            })
            .collect(),
        crate::project::MatchCertainty::Definite,
    )
    .unwrap()
}

#[test]
fn symbol_groups_preserve_order_and_merge_only_equal_symbols() {
    let mut evidence = vec![
        evidence("request", &[2, 4]),
        evidence("request", &[6]),
        evidence("other", &[8]),
    ];
    normalize_evidence(&mut evidence, Rule::EVIDENCE_LIMIT);
    assert_eq!(evidence.len(), 2);
    assert_eq!(evidence[0].symbol(), "request");
    assert_eq!(evidence[0].count(), 3);
    assert_eq!(evidence[1].symbol(), "other");
    assert_eq!(evidence[1].occurrences()[0].fact(), Some(8));
}

#[test]
fn truncation_preserves_exact_count_and_marker() {
    let mut evidence = vec![evidence(
        "request",
        &(0..(Rule::EVIDENCE_LIMIT + 4))
            .map(|value| u32::try_from(value).unwrap() + 2)
            .collect::<Vec<_>>(),
    )];
    normalize_evidence(&mut evidence, Rule::EVIDENCE_LIMIT);
    assert_eq!(evidence[0].count() as usize, Rule::EVIDENCE_LIMIT + 4);
    assert_eq!(evidence[0].occurrences().len(), Rule::EVIDENCE_LIMIT);
    assert!(evidence[0].is_truncated());
}
