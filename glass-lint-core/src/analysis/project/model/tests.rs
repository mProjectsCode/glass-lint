use super::*;

/// The frozen semantic model must be shareable across threads so that
/// future multi-threaded matcher projection is safe.
#[test]
fn semantic_model_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ProjectSemanticModel>();
}

/// The linking session must be sendable across threads as it owns only
/// Send types (ExportLookupCache).
#[test]
fn linking_session_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<LinkingSession>();
}
