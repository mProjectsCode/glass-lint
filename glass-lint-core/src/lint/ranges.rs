//! Source-range conversion and deterministic containment reduction.

use crate::diagnostic::SourceRange;

/// Remove ranges enclosed by an earlier, widest range in source order.
pub fn remove_contained_ranges(ranges: &mut Vec<SourceRange>) {
    ranges.sort_by(|left, right| {
        (left.start().line(), left.start().column())
            .cmp(&(right.start().line(), right.start().column()))
            .then_with(|| {
                (right.end().line(), right.end().column())
                    .cmp(&(left.end().line(), left.end().column()))
            })
    });
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
