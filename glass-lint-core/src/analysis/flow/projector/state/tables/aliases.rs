use std::collections::BTreeMap;

use crate::analysis::model::value::{FlowObjectId, ValueId};

#[derive(Debug, Default)]
pub(in crate::analysis::flow::projector) struct AliasTable {
    values: BTreeMap<ValueId, FlowObjectId>,
    /// Reverse index: how many ValueIds alias each FlowObjectId.
    object_refs: ObjectRefCounts,
}

impl AliasTable {
    pub(in crate::analysis::flow::projector) fn get(&self, value: ValueId) -> Option<FlowObjectId> {
        self.values.get(&value).copied()
    }

    pub(super) fn values(&self) -> impl Iterator<Item = &FlowObjectId> {
        self.values.values()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&ValueId, &FlowObjectId)> {
        self.values.iter()
    }

    pub(in crate::analysis::flow::projector) fn set(
        &mut self,
        value: ValueId,
        object: FlowObjectId,
    ) -> Option<FlowObjectId> {
        let previous = self.values.insert(value, object);
        if let Some(previous) = previous {
            self.object_refs.decrement(previous);
        }
        self.object_refs.increment(object);
        previous
    }

    pub(in crate::analysis::flow::projector) fn remove(
        &mut self,
        value: ValueId,
    ) -> Option<FlowObjectId> {
        let object = self.values.remove(&value)?;
        self.object_refs.decrement(object);
        Some(object)
    }

    pub(super) fn take(&mut self) -> BTreeMap<ValueId, FlowObjectId> {
        self.object_refs.clear();
        std::mem::take(&mut self.values)
    }

    pub(super) fn contains_object(&self, object: FlowObjectId) -> bool {
        self.object_refs.contains(object)
    }

    pub(super) fn objects(&self) -> impl Iterator<Item = FlowObjectId> + '_ {
        self.object_refs.keys()
    }
}

#[derive(Debug, Default)]
struct ObjectRefCounts(BTreeMap<FlowObjectId, usize>);

impl ObjectRefCounts {
    pub(in crate::analysis::flow::projector) fn clear(&mut self) {
        self.0.clear();
    }

    pub(in crate::analysis::flow::projector) fn increment(&mut self, object: FlowObjectId) {
        *self.0.entry(object).or_insert(0) += 1;
    }

    pub(in crate::analysis::flow::projector) fn decrement(&mut self, object: FlowObjectId) {
        if let Some(count) = self.0.get_mut(&object) {
            *count -= 1;
            if *count == 0 {
                self.0.remove(&object);
            }
        }
    }

    pub(in crate::analysis::flow::projector) fn contains(&self, object: FlowObjectId) -> bool {
        self.0.contains_key(&object)
    }

    pub(in crate::analysis::flow::projector) fn keys(
        &self,
    ) -> impl Iterator<Item = FlowObjectId> + '_ {
        self.0.keys().copied()
    }
}
