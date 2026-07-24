//! SCC decomposition and DAG construction.

use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::ModuleId;

/// Strongly connected components via deterministic iterative Kosaraju.
pub(super) fn strongly_connected_components(
    adjacency: &BTreeMap<ModuleId, Vec<ModuleId>>,
    nodes: impl IntoIterator<Item = ModuleId>,
) -> Vec<Vec<ModuleId>> {
    // Kosaraju's algorithm expressed as two sequential passes (forward order,
    // reverse DFS). Splitting into sub-functions would isolate the phases
    // but the shared `seen`/`order` state across both passes is clearest
    // when visible in one function body.
    let nodes = nodes.into_iter().collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    for node in nodes.iter().copied() {
        if seen.contains(&node) {
            continue;
        }
        let mut stack = vec![(node, false)];
        while let Some((current, expanded)) = stack.pop() {
            if expanded {
                order.push(current);
                continue;
            }
            if !seen.insert(current) {
                continue;
            }
            stack.push((current, true));
            for next in adjacency.get(&current).into_iter().flatten().rev().copied() {
                if !seen.contains(&next) {
                    stack.push((next, false));
                }
            }
        }
    }
    let mut reverse = adjacency.iter().fold(
        BTreeMap::<ModuleId, Vec<ModuleId>>::new(),
        |mut reverse, (from, tos)| {
            for to in tos {
                reverse.entry(*to).or_default().push(*from);
            }
            reverse
        },
    );
    for values in reverse.values_mut() {
        values.sort_unstable();
    }
    seen.clear();
    let mut components = Vec::new();
    for node in order.into_iter().rev() {
        if seen.contains(&node) {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![node];
        seen.insert(node);
        while let Some(current) = stack.pop() {
            component.push(current);
            for next in reverse.get(&current).into_iter().flatten().rev().copied() {
                if seen.insert(next) {
                    stack.push(next);
                }
            }
        }
        if !component.is_empty() {
            component.sort_unstable();
            components.push(component);
        }
    }
    components.sort();
    components
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

    let mut dag: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (from, targets) in forward {
        let Some(&from_scc) = module_to_scc.get(from) else {
            continue;
        };
        for to in targets {
            let Some(&to_scc) = module_to_scc.get(to) else {
                continue;
            };
            if from_scc != to_scc {
                let edges = dag.entry(from_scc).or_default();
                if !edges.contains(&to_scc) {
                    edges.push(to_scc);
                }
            }
        }
    }
    for edges in dag.values_mut() {
        edges.sort_unstable();
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
    (dag, order)
}
