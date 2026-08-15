use glass_lint_datastructures::{Position, SourceRange};

use super::*;

#[test]
fn retained_indices_keep_only_the_outermost_range() {
    let mut ranges = (1..=5_000)
        .map(|column| {
            SourceRange::new(
                Position::new(1, column).unwrap(),
                Position::new(2, 5_001 - column).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    ranges.push(ranges[0].clone());
    let entries = ranges
        .into_iter()
        .map(|range| EvidenceRangeEntry {
            range,
            occurrences: Vec::new(),
        })
        .collect::<Vec<_>>();

    assert_eq!(retained_indices(&entries), vec![0]);
}
