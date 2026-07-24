//! Deterministic containment reduction for finding source ranges.

use glass_lint_datastructures::SourceRange;

/// Remove ranges that are fully enclosed by an earlier, wider range in
/// source order. When ranges are equal, only the first survives.
/// Runs in O(n log n) dominated by the initial sort.
pub fn remove_contained_ranges(ranges: &mut Vec<SourceRange>) {
    ranges.sort_by(|left, right| {
        (left.start().line(), left.start().column())
            .cmp(&(right.start().line(), right.start().column()))
            .then_with(|| {
                (right.end().line(), right.end().column())
                    .cmp(&(left.end().line(), left.end().column()))
            })
    });

    // Sweep left-to-right keeping only ranges whose end extends past the
    // current enclosing range. Sorting guarantees that if range B starts
    // at or after the same position as range A, and B's end is before or
    // at A's end, then B is fully contained by A.
    let mut enclosing_end = None;

    ranges.retain(|range| {
        let end = (range.end().line(), range.end().column());
        if enclosing_end.is_some_and(|outer| end <= outer) {
            return false;
        }
        enclosing_end = Some(end);
        true
    });
}
