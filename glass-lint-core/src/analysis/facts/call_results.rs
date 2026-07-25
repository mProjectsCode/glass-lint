//! Named storage for call-site results emitted during one fact pass.

use std::collections::BTreeMap;

use swc_common::Span;

use crate::analysis::{lowering::ParserSpanKey, value::ValueId};

#[derive(Debug, Default)]
/// Deterministic per-pass table connecting call resolution with its emitted
/// call fact and later assignments that consume the returned value.
pub(super) struct CallResultTable(BTreeMap<ParserSpanKey, ValueId>);

impl CallResultTable {
    /// Look up the value identity previously assigned to a call span.
    pub(super) fn get(&self, span: Span) -> Option<ValueId> {
        self.0.get(&ParserSpanKey::from(span)).copied()
    }

    /// Associate a call span with its stable returned-object identity.
    pub(super) fn insert(&mut self, span: Span, value: ValueId) {
        self.0.insert(ParserSpanKey::from(span), value);
    }
}
