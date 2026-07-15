//! Owned state for project linking.

use std::collections::BTreeMap;

use super::super::{ExportResolution, ModuleId};
use crate::project::ResolutionRequestKey;

#[derive(Debug, Default)]
pub(in crate::analysis) struct ModuleGraph {
    forward: BTreeMap<ModuleId, Vec<ModuleId>>,
    reverse: BTreeMap<ModuleId, Vec<ModuleId>>,
    provenance: BTreeMap<(ModuleId, ModuleId), Vec<ResolutionRequestKey>>,
    components: Vec<Vec<ModuleId>>,
}
impl ModuleGraph {
    pub(in crate::analysis) fn ensure_node(&mut self, id: ModuleId) {
        self.forward.entry(id).or_default();
    }

    pub(in crate::analysis) fn insert_edge(
        &mut self,
        from: ModuleId,
        to: ModuleId,
        request: ResolutionRequestKey,
    ) -> bool {
        self.ensure_node(from);
        self.provenance.entry((from, to)).or_default().push(request);
        let targets = self.forward.entry(from).or_default();
        if targets.contains(&to) {
            return false;
        }
        targets.push(to);
        self.reverse.entry(to).or_default().push(from);
        true
    }

    pub(in crate::analysis) fn normalize(&mut self) {
        for values in self.forward.values_mut().chain(self.reverse.values_mut()) {
            values.sort_unstable();
            values.dedup();
        }
        for requests in self.provenance.values_mut() {
            requests.sort_unstable();
            requests.dedup();
        }
    }

    pub(in crate::analysis) fn forward(&self) -> &BTreeMap<ModuleId, Vec<ModuleId>> {
        &self.forward
    }

    pub(in crate::analysis) fn components(&self) -> &[Vec<ModuleId>] {
        &self.components
    }

    pub(in crate::analysis) fn set_components(&mut self, components: Vec<Vec<ModuleId>>) {
        self.components = components;
    }

    pub(in crate::analysis) fn edge_count(&self) -> usize {
        self.forward.values().map(Vec::len).sum()
    }
}

#[derive(Debug, Default)]
pub(in crate::analysis) struct ExportTable(BTreeMap<(ModuleId, String), ExportResolution>);
impl ExportTable {
    pub(in crate::analysis) fn resolve(
        &self,
        module: ModuleId,
        export: &str,
    ) -> Option<&ExportResolution> {
        self.0.get(&(module, export.to_owned()))
    }

    pub(in crate::analysis) fn set_monotone(
        &mut self,
        module: ModuleId,
        export: String,
        value: ExportResolution,
    ) -> bool {
        if self.0.get(&(module, export.clone())) == Some(&value) {
            return false;
        }
        self.0.insert((module, export), value);
        true
    }

    pub(in crate::analysis) fn mark_unknown(&mut self) {
        for value in self.0.values_mut() {
            *value = ExportResolution::Unknown;
        }
    }

    pub(in crate::analysis) fn len(&self) -> usize {
        self.0.len()
    }
}
