use swc_common::{SourceMap, Span, sync::Lrc};

use crate::diagnostic::SourceRange;

pub(crate) fn remove_contained_ranges(ranges: &mut Vec<SourceRange>) {
    ranges.sort_by(|left, right| {
        (left.start.line, left.start.column)
            .cmp(&(right.start.line, right.start.column))
            .then_with(|| (right.end.line, right.end.column).cmp(&(left.end.line, left.end.column)))
    });
    let mut enclosing_end = None;

    ranges.retain(|range| {
        let end = (range.end.line, range.end.column);
        if enclosing_end.is_some_and(|outer| end <= outer) {
            return false;
        }
        enclosing_end = Some(end);
        true
    });
}

pub(crate) fn source_range_from_span(source_map: &Lrc<SourceMap>, span: Span) -> SourceRange {
    let start = source_map.lookup_char_pos(span.lo());
    let end = source_map.lookup_char_pos(span.hi());

    SourceRange {
        start: crate::diagnostic::Position {
            line: start.line.try_into().unwrap_or(u32::MAX),
            column: start
                .col_display
                .try_into()
                .unwrap_or(u32::MAX)
                .saturating_add(1),
        },
        end: crate::diagnostic::Position {
            line: end.line.try_into().unwrap_or(u32::MAX),
            column: end
                .col_display
                .try_into()
                .unwrap_or(u32::MAX)
                .saturating_add(1),
        },
    }
}

pub(crate) fn source_range(source: &str, start: usize, length: usize) -> SourceRange {
    SourceRange::from_source(source, start, length)
}
