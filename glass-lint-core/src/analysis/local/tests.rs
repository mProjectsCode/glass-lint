use std::sync::Arc;

use super::*;

fn assert_send_sync<T: Send + Sync>() {}

fn test_key(text: &str, version: &'static str) -> ArtifactCacheKey {
    let source = crate::project::SourceFile::new("test.js", text).unwrap();
    let env = crate::Environment::default();
    let limits = crate::AnalysisLimits::default();
    ArtifactCacheKey::for_engine_version(&source, &env, &limits, version)
}

fn empty_shared_artifact() -> SharedSemanticArtifact {
    SharedSemanticArtifact {
        semantic: Arc::new(SemanticArtifact::from_analysis(
            crate::analysis::facts::SemanticFacts::default(),
            BTreeMap::new(),
            usize::MAX,
            DerivedPhaseCapabilities::enabled(),
            crate::analysis::semantic::status::LocalAnalysisStatus::default(),
        )),
        source_index: Arc::new(SourceLineIndex::new("")),
    }
}

#[test]
fn local_artifact_is_send_sync_and_cloneable() {
    assert_send_sync::<LocalArtifact>();
    assert_send_sync::<SemanticArtifact>();
}

#[test]
fn function_effects_are_derived_only_when_requested() {
    let artifact = SemanticArtifact::from_analysis(
        crate::analysis::facts::SemanticFacts::default(),
        BTreeMap::new(),
        usize::MAX,
        DerivedPhaseCapabilities::enabled(),
        crate::analysis::semantic::status::LocalAnalysisStatus::default(),
    );
    assert!(!artifact.effects_initialized());
    let _ = artifact.effects();
    assert!(artifact.effects_initialized());
}

#[test]
fn source_context_reuses_one_line_index() {
    let source = crate::project::SourceFile::new("main.js", "fetch('/');").unwrap();
    let context = LocatedSourceContext::new(&source);
    let cloned = context.clone();
    assert!(Arc::ptr_eq(&context.lines, &cloned.lines));
    assert_eq!(Arc::strong_count(&context.lines), 2);
}

#[test]
fn artifact_cache_insert_then_get_hit() {
    let mut cache = ArtifactCache::default();
    let key = test_key("x = 1;", "1.0.0");
    let artifact = empty_shared_artifact();
    assert!(cache.get(&key).is_none());
    cache.insert(key.clone(), artifact);
    let retrieved = cache.get(&key);
    assert!(retrieved.is_some());
}

#[test]
fn artifact_cache_evicts_oldest_when_full() {
    let mut cache = ArtifactCache::default();
    let mut keys = Vec::new();
    for i in 0..ArtifactCache::MAX_ENTRIES + 5 {
        let text = format!("x = {i};");
        let key = test_key(&text, "1.0.0");
        let artifact = empty_shared_artifact();
        let evicted = cache.insert(key.clone(), artifact);
        keys.push(key);
        if i >= ArtifactCache::MAX_ENTRIES {
            assert!(evicted, "insert {i} should evict oldest");
        }
    }
    let oldest_key = &keys[0];
    assert!(
        cache.get(oldest_key).is_none(),
        "oldest entry should be evicted"
    );
    let newest_key = keys.last().unwrap();
    assert!(
        cache.get(newest_key).is_some(),
        "newest entry should be present"
    );
}

#[test]
fn artifact_cache_replacement_does_not_evict() {
    let mut cache = ArtifactCache::default();
    let key = test_key("x = 1;", "1.0.0");
    let artifact_a = empty_shared_artifact();
    let artifact_b = empty_shared_artifact();
    cache.insert(key.clone(), artifact_a);
    let evicted = cache.insert(key, artifact_b);
    assert!(!evicted, "replacing exact key should not evict");
}

#[test]
fn artifact_cache_miss_on_different_key() {
    let mut cache = ArtifactCache::default();
    let key_a = test_key("x = 1;", "1.0.0");
    let key_b = test_key("y = 2;", "1.0.0");
    let artifact = empty_shared_artifact();
    cache.insert(key_a, artifact);
    assert!(cache.get(&key_b).is_none(), "different key should not hit");
}
