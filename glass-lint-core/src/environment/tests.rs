use glass_lint_datastructures::SymbolPath;

use super::*;

#[test]
fn defaults_are_host_independent_and_extensions_are_additive() {
    let mut environment = Environment::default();
    assert!(environment.is_global("Math"));
    assert!(
        environment
            .global_objects()
            .any(|name| name == "globalThis")
    );
    assert!(!environment.is_global("fetch"));
    assert!(!environment.global_objects().any(|name| name == "window"));

    environment.add_global("fetch").unwrap();
    environment.add_global_object("activeWindow").unwrap();
    assert!(environment.is_global("fetch"));
    assert!(environment.is_global("activeWindow"));
    assert!(
        environment
            .global_objects()
            .any(|name| name == "activeWindow")
    );
}

#[test]
fn restricted_global_objects_do_not_inherit_current_realm_injections() {
    let mut environment = Environment::default();
    environment.add_global("requestUrl").unwrap();
    environment
        .add_global_object_with_members("activeWindow", ["eval", "fetch"])
        .unwrap();

    assert!(environment.is_global_member("activeWindow", "eval"));
    assert!(environment.is_global_member("activeWindow", "fetch"));
    assert!(!environment.is_global_member("activeWindow", "requestUrl"));
    assert!(environment.is_global_member("globalThis", "requestUrl"));
}

#[test]
fn rejects_paths_and_other_non_identifiers() {
    let mut environment = Environment::default();
    assert!(environment.add_global("window.fetch").is_err());
    assert!(environment.add_global_object("").is_err());
}

#[test]
fn extend_merges_bindings_and_objects() {
    let mut base = Environment::default();
    base.add_global("alpha").unwrap();
    base.add_global_object("win1").unwrap();

    let mut other = Environment::default();
    other.add_global("beta").unwrap();
    other.add_global_object("win2").unwrap();

    base.extend(&other);
    assert!(base.is_global("alpha"));
    assert!(base.is_global("beta"));
    assert!(base.global_objects().any(|n| n == "win1"));
    assert!(base.global_objects().any(|n| n == "win2"));
}

#[test]
fn extend_configured_globals_wins_over_restricted() {
    let mut base = Environment::default();
    base.add_global_object_with_members("shared", ["fetch"])
        .unwrap();

    let mut other = Environment::default();
    other.add_global_object("shared").unwrap();

    base.extend(&other);
    // After extend, "shared" becomes ConfiguredGlobals, so members
    // resolve against global bindings. "fetch" is not a default global.
    assert!(!base.is_global_member("shared", "fetch"));
    assert!(base.is_global_member("shared", "Array"));
}

#[test]
fn global_object_aliases_match_configured_globals() {
    let mut env = Environment::default();
    env.add_global_object("window").unwrap();
    env.add_global_object("self").unwrap();
    env.add_global_object_with_members("foreign", ["eval"])
        .unwrap();

    assert!(env.global_object_aliases_match("window", "self"));
    assert!(!env.global_object_aliases_match("window", "foreign"));
    assert!(env.global_object_aliases_match("window", "window"));
}

#[test]
fn global_object_name_paths_match_aliases_and_promoted_members() {
    let mut env = Environment::default();
    env.add_global("fetch").unwrap();
    env.add_global_object("window").unwrap();
    env.add_global_object("self").unwrap();

    let mut names = NameTable::default();
    for name in ["window", "self", "fetch"] {
        names.intern(name).unwrap();
    }
    let window_fetch = names
        .lookup_path(&SymbolPath::from_chain("window.fetch"))
        .unwrap();
    let self_fetch = names
        .lookup_path(&SymbolPath::from_chain("self.fetch"))
        .unwrap();
    let fetch = names.lookup_path(&SymbolPath::from_chain("fetch")).unwrap();

    assert!(env.global_object_name_paths_match(&window_fetch, &self_fetch, &names));
    assert!(env.global_object_name_paths_match(&window_fetch, &fetch, &names));
}

#[test]
fn global_object_name_paths_match_identical_paths() {
    let env = Environment::default();
    let mut names = NameTable::default();
    names.intern("Math").unwrap();
    let path = names.lookup_path(&SymbolPath::from_chain("Math")).unwrap();
    assert!(env.global_object_name_paths_match(&path, &path, &names));
}

#[test]
fn global_object_name_paths_match_rejects_different_paths() {
    let env = Environment::default();
    let mut names = NameTable::default();
    names.intern("Math").unwrap();
    names.intern("JSON").unwrap();
    let left = names.lookup_path(&SymbolPath::from_chain("Math")).unwrap();
    let right = names.lookup_path(&SymbolPath::from_chain("JSON")).unwrap();
    assert!(!env.global_object_name_paths_match(&left, &right, &names));
}

#[test]
fn fingerprint_is_deterministic() {
    let mut a = Environment::default();
    a.add_globals(["fetch", "navigator"]).unwrap();
    let mut b = Environment::default();
    b.add_globals(["navigator", "fetch"]).unwrap();

    let mut ha = Fingerprint::init();
    let mut hb = Fingerprint::init();
    a.write_fingerprint_bytes(&mut ha);
    b.write_fingerprint_bytes(&mut hb);
    assert_eq!(ha.into_raw(), hb.into_raw());
}

#[test]
fn fingerprint_differs_for_different_environments() {
    let mut a = Environment::default();
    a.add_global("fetch").unwrap();
    let b = Environment::default();

    let mut ha = Fingerprint::init();
    let mut hb = Fingerprint::init();
    a.write_fingerprint_bytes(&mut ha);
    b.write_fingerprint_bytes(&mut hb);
    assert_ne!(ha.into_raw(), hb.into_raw());
}

#[test]
fn global_bindings_iterator_returns_configured_names() {
    let mut env = Environment::default();
    env.add_globals(["alpha", "beta"]).unwrap();
    let names: Vec<&str> = env.global_bindings().collect();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
    assert!(names.contains(&"Math"));
}

#[test]
fn bulk_global_registration_is_atomic_on_validation_failure() {
    let mut env = Environment::default();
    assert!(env.add_globals(["alpha", ""]).is_err());
    assert!(!env.global_bindings().any(|name| name == "alpha"));
}
