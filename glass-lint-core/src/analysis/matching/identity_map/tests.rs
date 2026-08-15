use super::*;

fn external(module: &str, export: &str) -> ExportResolution {
    ExportResolution::External {
        module: module.into(),
        export: export.into(),
    }
}

#[test]
fn star_merge_marks_disagreeing_identities_ambiguous() {
    let key = ModuleExportKey::new("pkg", "request");
    let mut merged = ModuleIdentityMap::new();
    merged.insert(key.clone(), external("a", "request"));

    let mut other = ModuleIdentityMap::new();
    other.insert(key.clone(), external("b", "request"));
    merged.merge_star_from(other);

    assert_eq!(merged.get(&key), Some(&ExportResolution::Ambiguous));
}

#[test]
fn missing_merge_preserves_authoritative_identity() {
    let key = ModuleExportKey::new("pkg", "request");
    let mut merged = ModuleIdentityMap::new();
    merged.insert(key.clone(), external("direct", "request"));

    let mut other = ModuleIdentityMap::new();
    other.insert(key.clone(), external("star", "request"));
    merged.merge_missing_from(other);

    assert_eq!(merged.get(&key), Some(&external("direct", "request")));
}

#[test]
fn unknown_wildcard_masks_unresolved_namespace_members() {
    let mut identities = ModuleIdentityMap::new();
    identities.insert(
        ModuleExportKey::wildcard("namespace"),
        ExportResolution::Unknown,
    );

    assert_eq!(
        identities.get(&ModuleExportKey::wildcard("namespace")),
        Some(&ExportResolution::Unknown)
    );
    assert_eq!(
        identities.get(&ModuleExportKey::new("namespace", "request")),
        None
    );
}
