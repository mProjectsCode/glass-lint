//! SCC decomposition and DAG construction using petgraph.

use std::collections::{BTreeMap, BTreeSet};

use petgraph::{algo::kosaraju_scc, graph::DiGraph};

use crate::analysis::ModuleId;

/// Strongly connected components via petgraph's kosaraju_scc.
/// Components are sorted internally for deterministic output.
pub(super) fn strongly_connected_components(
    adjacency: &BTreeMap<ModuleId, Vec<ModuleId>>,
    nodes: impl IntoIterator<Item = ModuleId>,
) -> Vec<Vec<ModuleId>> {
    let nodes: Vec<ModuleId> = nodes.into_iter().collect();
    let mut graph = DiGraph::<ModuleId, ()>::new();
    let mut node_indices = BTreeMap::new();

    for &node in &nodes {
        let idx = graph.add_node(node);
        node_indices.insert(node, idx);
    }

    for (from, targets) in adjacency {
        let Some(&from_idx) = node_indices.get(from) else {
            continue;
        };
        for &to in targets {
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

/// Build the SCC DAG and topological order from the original graph edges and
/// component decomposition.
pub(super) fn build_scc_dag_and_order(
    forward: &BTreeMap<ModuleId, Vec<ModuleId>>,
    components: &[Vec<ModuleId>],
) -> (BTreeMap<usize, Vec<usize>>, Vec<usize>) {
    let module_to_scc: BTreeMap<ModuleId, usize> = components
        .iter()
        .enumerate()
        .flat_map(|(idx, component)| component.iter().map(move |&m| (m, idx)))
        .collect();

    // BTreeSet avoids quadratic deduplication from Vec::contains
    let mut dag: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (from, targets) in forward {
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
    let dag: BTreeMap<usize, Vec<usize>> = dag
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect();

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
    (dag, order)
}
