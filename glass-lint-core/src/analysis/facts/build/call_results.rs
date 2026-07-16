//! Named storage for call-site results emitted during one fact pass.

use std::collections::BTreeMap;

use swc_common::Span;

use crate::analysis::value::ValueId;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
/// Source-span key used to reuse one value identity for one call site.
pub(super) struct CallSiteKey {
    lo: u32,
    hi: u32,
}

impl CallSiteKey {
    /// Use source coordinates as the stable identity because one call span is
    /// shared by result resolution and the emitted call fact.
    pub(super) fn from_span(span: Span) -> Self {
        Self {
            lo: span.lo.0,
            hi: span.hi.0,
        }
    }
}

#[derive(Debug, Default)]
/// Deterministic per-pass table connecting call resolution with its emitted
/// call fact and later assignments that consume the returned value.
pub(super) struct CallResultTable(BTreeMap<CallSiteKey, ValueId>);

impl CallResultTable {
    /// Look up the value identity previously assigned to a call span.
    pub(super) fn get(&self, span: Span) -> Option<ValueId> {
        self.0.get(&CallSiteKey::from_span(span)).copied()
    }

    /// Associate a call span with its stable returned-object identity.
    pub(super) fn insert(&mut self, span: Span, value: ValueId) {
        self.0.insert(CallSiteKey::from_span(span), value);
    }
}
