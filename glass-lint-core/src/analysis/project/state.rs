//! Owned state for project linking.

use std::collections::BTreeMap;

use smol_str::SmolStr;

use crate::analysis::{ExportResolution, ModuleId};

#[derive(Debug, Default)]
/// Deterministic internal-module graph.
pub(in crate::analysis) struct ModuleGraph {
    /// Outgoing internal edges by importer.
    forward: BTreeMap<ModuleId, Vec<ModuleId>>,
}
impl ModuleGraph {
    /// Ensure a module appears even when it has no internal dependencies.
    pub(in crate::analysis) fn ensure_node(&mut self, id: ModuleId) {
        self.forward.entry(id).or_default();
    }

    /// Insert one internal edge.
    pub(in crate::analysis) fn insert_edge(&mut self, from: ModuleId, to: ModuleId) -> bool {
        self.ensure_node(from);
        let targets = self.forward.entry(from).or_default();
        if targets.contains(&to) {
            return false;
        }
        targets.push(to);
        true
    }

    /// Sort and deduplicate all graph collections for deterministic output.
    pub(in crate::analysis) fn normalize(&mut self) {
        for values in self.forward.values_mut() {
            values.sort_unstable();
            values.dedup();
        }
    }

    /// Borrow outgoing adjacency for graph traversal.
    pub(in crate::analysis) fn forward(&self) -> &BTreeMap<ModuleId, Vec<ModuleId>> {
        &self.forward
    }

    /// Count unique outgoing internal edges.
    pub(in crate::analysis) fn edge_count(&self) -> usize {
        self.forward.values().map(Vec::len).sum()
    }
}

/// Strongly connected component partition, DAG, and topological order.
#[derive(Debug)]
pub(in crate::analysis) struct SccPartition {
    pub components: Vec<Vec<ModuleId>>,
    #[allow(dead_code)]
    pub dag: BTreeMap<usize, Vec<usize>>,
    pub order: Vec<usize>,
}

#[derive(Debug, Default)]
/// Resolved export identities for one module.
pub(in crate::analysis) struct ModuleExports(BTreeMap<SmolStr, ExportResolution>);
impl ModuleExports {
    pub fn get(&self, name: &SmolStr) -> Option<&ExportResolution> {
        self.0.get(name)
    }

    pub fn insert(&mut self, name: SmolStr, value: ExportResolution) -> Option<ExportResolution> {
        self.0.insert(name, value)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&SmolStr, &ExportResolution)> {
        self.0.iter()
    }
}

#[derive(Debug, Default)]
/// Qualified export identities indexed by module and export name.
pub(in crate::analysis) struct ExportTable(BTreeMap<ModuleId, ModuleExports>);
impl ExportTable {
    /// Look up the current fixed-point value for one export.
    pub(in crate::analysis) fn resolve(
        &self,
        module: ModuleId,
        export: &SmolStr,
    ) -> Option<&ExportResolution> {
        self.0.get(&module)?.get(export)
    }

    /// Store a changed export identity and report whether it changed.
    pub(in crate::analysis) fn set_monotone(
        &mut self,
        module: ModuleId,
        export: &SmolStr,
        value: ExportResolution,
    ) -> bool {
        let entry = self.0.entry(module).or_default();

        if entry.get(export) == Some(&value) {
            return false;
        }
        entry.insert(export.clone(), value);
        true
    }

    /// Return the total number of resolved module/export entries.
    pub(in crate::analysis) fn len(&self) -> usize {
        self.0.values().map(ModuleExports::len).sum()
    }

    /// Borrow the resolved exports for one module.
    pub(in crate::analysis) fn module_exports(&self, module: ModuleId) -> Option<&ModuleExports> {
        self.0.get(&module)
    }
}
