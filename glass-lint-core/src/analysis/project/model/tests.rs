use super::*;

/// The frozen semantic model must be shareable across threads so that
/// future multi-threaded matcher projection is safe.
#[test]
fn semantic_model_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ProjectSemanticModel>();
}

/// The export lookup cache must be sendable across threads as it owns only
/// Send types (a BTreeMap of QualifiedExportId and a capacity bound).
#[test]
fn export_lookup_cache_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<ExportLookupCache>();
}
