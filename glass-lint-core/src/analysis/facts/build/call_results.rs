//! Named storage for call-site results emitted during one fact pass.

use std::collections::BTreeMap;

use swc_common::Span;

use crate::analysis::value::ValueId;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct CallSiteKey {
    lo: u32,
    hi: u32,
}

impl CallSiteKey {
    pub(super) fn from_span(span: Span) -> Self {
        Self {
            lo: span.lo.0,
            hi: span.hi.0,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct CallResultTable(BTreeMap<CallSiteKey, ValueId>);

impl CallResultTable {
    pub(super) fn get(&self, span: Span) -> Option<ValueId> {
        self.0.get(&CallSiteKey::from_span(span)).copied()
    }

    pub(super) fn insert(&mut self, span: Span, value: ValueId) {
        self.0.insert(CallSiteKey::from_span(span), value);
    }
}
