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
    let graph = graph.normalize();
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
    let graph = graph.normalize();

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
    let graph = graph.normalize();

    assert!(graph.scc_partition(2).is_none());
}

#[test]
fn default_partition_iterates_nothing() {
    let partition = SccPartition::default();
    assert!(partition.is_empty());
    assert_eq!(partition.ordered_components().count(), 0);
}

#[test]
fn export_lookup_cache_keys_include_module_identity() {
    let mut cache = ExportLookupCache::new(2);
    let first = QualifiedExportId::new(module(0), "value");
    let second = QualifiedExportId::new(module(1), "value");
    cache.insert(first.clone(), Some(ExportResolution::Unknown));
    cache.insert(second.clone(), None);

    assert!(matches!(
        cache.lookup(&first),
        ExportLookupCacheResult::Hit(Some(ExportResolution::Unknown))
    ));
    assert!(matches!(
        cache.lookup(&second),
        ExportLookupCacheResult::Hit(None)
    ));
}

#[test]
fn export_lookup_cache_capacity_uses_unique_map_keys() {
    let mut cache = ExportLookupCache::new(1);
    let first = QualifiedExportId::new(module(0), "first");
    let second = QualifiedExportId::new(module(0), "second");
    cache.insert(first.clone(), None);
    cache.insert(second.clone(), Some(ExportResolution::Unknown));

    assert!(matches!(
        cache.lookup(&first),
        ExportLookupCacheResult::Hit(None)
    ));
    assert!(matches!(
        cache.lookup(&second),
        ExportLookupCacheResult::Miss
    ));
}

#[test]
fn export_table_resolution_replacement_tracks_entry_count() {
    let mut table = ExportTable::default();
    let id = QualifiedExportId::new(module(0), "value");

    assert_eq!(
        table.set_resolution(id.clone(), ExportResolution::Unknown),
        ExportUpdate::Inserted
    );
    assert_eq!(table.len(), 1);
    assert_eq!(
        table.set_resolution(id.clone(), ExportResolution::Unknown),
        ExportUpdate::Unchanged
    );
    assert_eq!(
        table.set_resolution(
            id.clone(),
            ExportResolution::Global {
                name: "fetch".into(),
            },
        ),
        ExportUpdate::Replaced
    );
    assert_eq!(table.len(), 1);
    assert_eq!(
        table.resolve(&id),
        Some(&ExportResolution::Global {
            name: "fetch".into()
        })
    );
}
