//! Owned state for project linking.
//!
//! The graph stores adjacency separately from request provenance so edge
//! deduplication cannot discard the source locations that explain a link.

use std::collections::BTreeMap;

use super::super::{ExportResolution, ModuleId};
use crate::project::ResolutionRequestKey;

#[derive(Debug, Default)]
/// Deterministic internal-module graph and its SCC decomposition.
pub(in crate::analysis) struct ModuleGraph {
    /// Outgoing internal edges by importer.
    forward: BTreeMap<ModuleId, Vec<ModuleId>>,
    /// Authored request keys that justify each collapsed edge.
    provenance: BTreeMap<(ModuleId, ModuleId), Vec<ResolutionRequestKey>>,
    /// Sorted strongly connected components of the graph.
    components: Vec<Vec<ModuleId>>,
}
impl ModuleGraph {
    /// Ensure a module appears even when it has no internal dependencies.
    pub(in crate::analysis) fn ensure_node(&mut self, id: ModuleId) {
        self.forward.entry(id).or_default();
    }

    /// Insert one internal edge and retain its request provenance.
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
        true
    }

    /// Sort and deduplicate all graph collections for deterministic output.
    pub(in crate::analysis) fn normalize(&mut self) {
        for values in self.forward.values_mut() {
            values.sort_unstable();
            values.dedup();
        }
        for requests in self.provenance.values_mut() {
            requests.sort_unstable();
            requests.dedup();
        }
    }

    /// Borrow outgoing adjacency for graph traversal.
    pub(in crate::analysis) fn forward(&self) -> &BTreeMap<ModuleId, Vec<ModuleId>> {
        &self.forward
    }

    /// Borrow the computed SCCs.
    pub(in crate::analysis) fn components(&self) -> &[Vec<ModuleId>] {
        &self.components
    }

    /// Store the sorted SCC decomposition produced by the linker.
    pub(in crate::analysis) fn set_components(&mut self, components: Vec<Vec<ModuleId>>) {
        self.components = components;
    }

    /// Count unique outgoing internal edges.
    pub(in crate::analysis) fn edge_count(&self) -> usize {
        self.forward.values().map(Vec::len).sum()
    }
}

#[derive(Debug, Default)]
/// Qualified export identities indexed by module and export name.
pub(in crate::analysis) struct ExportTable(BTreeMap<(ModuleId, String), ExportResolution>);
impl ExportTable {
    /// Look up the current fixed-point value for one export.
    pub(in crate::analysis) fn resolve(
        &self,
        module: ModuleId,
        export: &str,
    ) -> Option<&ExportResolution> {
        self.0.get(&(module, export.to_owned()))
    }

    /// Store a changed export identity and report whether it changed.
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

    /// Fail closed for every known export after linker non-convergence.
    pub(in crate::analysis) fn mark_unknown(&mut self) {
        for value in self.0.values_mut() {
            *value = ExportResolution::Unknown;
        }
    }

    /// Return the number of indexed module/export entries.
    pub(in crate::analysis) fn len(&self) -> usize {
        self.0.len()
    }
}
