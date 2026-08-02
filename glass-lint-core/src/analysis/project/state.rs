//! Owned state for project linking.

use std::collections::{BTreeMap, BTreeSet};

use petgraph::{algo::kosaraju_scc, graph::DiGraph};
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

    /// Insert one internal edge. Duplicates are removed by `normalize`.
    pub(in crate::analysis) fn insert_edge(&mut self, from: ModuleId, to: ModuleId) {
        self.ensure_node(from);
        let targets = self.forward.entry(from).or_default();
        targets.push(to);
    }

    /// Sort and deduplicate all graph collections for deterministic output.
    pub(in crate::analysis) fn normalize(&mut self) {
        for values in self.forward.values_mut() {
            values.sort_unstable();
            values.dedup();
        }
    }

    /// Iterate over the outgoing internal neighbors of one module.
    pub(in crate::analysis) fn neighbors(
        &self,
        from: ModuleId,
    ) -> impl Iterator<Item = ModuleId> + '_ {
        self.forward.get(&from).into_iter().flatten().copied()
    }

    /// Decompose into strongly connected components and a deterministic
    /// topological order. Returns `None` when a component exceeds the bound.
    pub(in crate::analysis) fn scc_partition(&self, max_scc_size: usize) -> Option<SccPartition> {
        let components = self.components();
        if components
            .iter()
            .any(|component| component.len() > max_scc_size)
        {
            return None;
        }
        let order = self.topological_order(&components);
        Some(SccPartition { components, order })
    }

    /// Strongly connected components via petgraph's kosaraju_scc. Components
    /// are sorted internally for deterministic output.
    fn components(&self) -> Vec<Vec<ModuleId>> {
        let mut graph = DiGraph::<ModuleId, ()>::new();
        let mut node_indices = BTreeMap::new();
        for &node in self.forward.keys() {
            let idx = graph.add_node(node);
            node_indices.insert(node, idx);
        }
        for (&from, &from_idx) in &node_indices {
            for to in self.neighbors(from) {
                let Some(&to_idx) = node_indices.get(&to) else {
                    continue;
                };
                graph.add_edge(from_idx, to_idx, ());
            }
        }
        let scc_result = kosaraju_scc(&graph);
        scc_result
            .into_iter()
            .map(|scc| {
                let mut members: Vec<ModuleId> = scc.into_iter().map(|idx| graph[idx]).collect();
                members.sort_unstable();
                members
            })
            .collect()
    }

    /// Build the SCC topological order from the graph edges and the component
    /// decomposition.
    fn topological_order(&self, components: &[Vec<ModuleId>]) -> Vec<usize> {
        let module_to_scc: BTreeMap<ModuleId, usize> = components
            .iter()
            .enumerate()
            .flat_map(|(idx, component)| component.iter().map(move |&m| (m, idx)))
            .collect();

        // BTreeSet avoids quadratic deduplication from Vec::contains.
        let mut dag: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
        for (from, targets) in &self.forward {
            let Some(&from_scc) = module_to_scc.get(from) else {
                continue;
            };
            for &to in targets {
                let Some(&to_scc) = module_to_scc.get(&to) else {
                    continue;
                };
                if from_scc != to_scc {
                    dag.entry(from_scc).or_default().insert(to_scc);
                }
            }
        }

        let scc_count = components.len();
        let mut in_degree = vec![0usize; scc_count];
        for targets in dag.values() {
            for &target in targets {
                in_degree[target] = in_degree[target].saturating_add(1);
            }
        }

        let mut queue: Vec<usize> = (0..scc_count).filter(|&i| in_degree[i] == 0).collect();
        let mut order = Vec::with_capacity(scc_count);
        while let Some(scc_idx) = queue.pop() {
            order.push(scc_idx);
            if let Some(targets) = dag.get(&scc_idx) {
                for &target in targets {
                    in_degree[target] = in_degree[target].saturating_sub(1);
                    if in_degree[target] == 0 {
                        queue.push(target);
                    }
                }
            }
        }

        order.reverse();
        order
    }

    /// Count unique outgoing internal edges.
    pub(in crate::analysis) fn edge_count(&self) -> usize {
        self.forward.values().map(Vec::len).sum()
    }
}

/// Strongly connected component partition, DAG, and topological order.
#[derive(Debug, Default)]
pub(in crate::analysis) struct SccPartition {
    components: Vec<Vec<ModuleId>>,
    order: Vec<usize>,
}
impl SccPartition {
    /// Iterate over components in topological order.
    pub(in crate::analysis) fn ordered_components(&self) -> impl Iterator<Item = &[ModuleId]> + '_ {
        self.order
            .iter()
            .map(move |&index| self.components[index].as_slice())
    }

    /// Return whether the partition has no ordered components.
    pub(in crate::analysis) fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
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

    pub fn iter(&self) -> impl Iterator<Item = (&SmolStr, &ExportResolution)> {
        self.0.iter()
    }
}

#[derive(Debug, Default)]
/// Qualified export identities indexed by module and export name.
pub(in crate::analysis) struct ExportTable {
    exports: BTreeMap<ModuleId, ModuleExports>,
    total_entries: usize,
}
impl ExportTable {
    /// Look up the current fixed-point value for one export.
    pub(in crate::analysis) fn resolve(
        &self,
        module: ModuleId,
        export: &SmolStr,
    ) -> Option<&ExportResolution> {
        self.exports.get(&module)?.get(export)
    }

    /// Store a changed export identity and report whether it changed.
    pub(in crate::analysis) fn set_monotone(
        &mut self,
        module: ModuleId,
        export: &SmolStr,
        value: ExportResolution,
    ) -> bool {
        let entry = self.exports.entry(module).or_default();

        if entry.get(export) == Some(&value) {
            return false;
        }
        let is_new = entry.get(export).is_none();
        entry.insert(export.clone(), value);
        if is_new {
            self.total_entries = self.total_entries.saturating_add(1);
        }
        true
    }

    /// Return the total number of resolved module/export entries.
    pub(in crate::analysis) fn len(&self) -> usize {
        self.total_entries
    }

    /// Borrow the resolved exports for one module.
    pub(in crate::analysis) fn module_exports(&self, module: ModuleId) -> Option<&ModuleExports> {
        self.exports.get(&module)
    }
}

#[derive(Debug)]
pub(in crate::analysis) struct LinkingSession {
    pub(super) lookup_cache: ExportLookupCache,
}

impl LinkingSession {
    pub fn new(capacity: usize) -> Self {
        Self {
            lookup_cache: ExportLookupCache::new(capacity),
        }
    }
}

#[derive(Debug)]
pub(in crate::analysis) struct ExportLookupCache {
    entries: BTreeMap<ModuleId, BTreeMap<SmolStr, Option<ExportResolution>>>,
    capacity: usize,
    count: usize,
}

impl ExportLookupCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            capacity,
            count: 0,
        }
    }

    pub fn get(&self, module: ModuleId, name: &SmolStr) -> Option<&Option<ExportResolution>> {
        self.entries.get(&module)?.get(name)
    }

    pub fn insert(&mut self, module: ModuleId, name: SmolStr, value: Option<ExportResolution>) {
        if self.count >= self.capacity {
            return;
        }
        let inner = self.entries.entry(module).or_default();
        if !inner.contains_key(&name) {
            self.count = self.count.saturating_add(1);
        }
        inner.insert(name, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(value: u32) -> ModuleId {
        ModuleId::new(value)
    }

    #[test]
    fn neighbors_iterate_sorted_outgoing_edges() {
        let mut graph = ModuleGraph::default();
        graph.insert_edge(module(0), module(1));
        graph.insert_edge(module(0), module(2));
        graph.insert_edge(module(2), module(1));
        graph.normalize();
        assert_eq!(
            graph.neighbors(module(0)).collect::<Vec<_>>(),
            vec![module(1), module(2)]
        );
        assert_eq!(graph.neighbors(module(1)).count(), 0);
        assert_eq!(
            graph.neighbors(module(2)).collect::<Vec<_>>(),
            vec![module(1)]
        );
    }

    #[test]
    fn scc_partition_groups_cycles_and_orders_dependencies_first() {
        let mut graph = ModuleGraph::default();
        graph.ensure_node(module(0));
        graph.ensure_node(module(1));
        graph.ensure_node(module(2));
        graph.insert_edge(module(0), module(1));
        graph.insert_edge(module(0), module(2));
        graph.insert_edge(module(2), module(1));
        graph.normalize();

        let partition = graph.scc_partition(4).expect("no oversized component");
        let order: Vec<Vec<ModuleId>> = partition
            .ordered_components()
            .map(<[ModuleId]>::to_vec)
            .collect();
        assert_eq!(
            order,
            vec![vec![module(1)], vec![module(2)], vec![module(0)]]
        );
        assert!(!partition.is_empty());
    }

    #[test]
    fn scc_partition_rejects_oversized_component() {
        let mut graph = ModuleGraph::default();
        graph.ensure_node(module(0));
        graph.ensure_node(module(1));
        graph.ensure_node(module(2));
        graph.insert_edge(module(0), module(1));
        graph.insert_edge(module(1), module(2));
        graph.insert_edge(module(2), module(0));
        graph.normalize();

        assert!(graph.scc_partition(2).is_none());
    }

    #[test]
    fn default_partition_iterates_nothing() {
        let partition = SccPartition::default();
        assert!(partition.is_empty());
        assert_eq!(partition.ordered_components().count(), 0);
    }
}
