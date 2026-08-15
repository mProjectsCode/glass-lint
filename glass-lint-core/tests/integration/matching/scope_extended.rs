use super::*;

#[test]
fn loop_continue_excludes_nonmatching_iteration() {
    assert_count(
        "let api = host.files; let i = 0; while (i < 10) { i++; api = local.files; continue; api.read(); } api.read();",
        rooted_read_rule(),
        1,
    );
}

/// Definite: the only reaching path after return has the identity.
#[test]
fn definite_abrupt_return_excludes_nonmatching_path() {
    assert_count(
        "function run(flag) { let api = local.files; if (flag) api = host.files; else return; api.read(); }",
        rooted_read_rule(),
        1,
    );
}

/// Definite: the only reaching path after throw has the identity.
#[test]
fn definite_abrupt_throw_excludes_nonmatching_path() {
    assert_count(
        "function run(flag) { let api = local.files; if (flag) api = host.files; else throw new Error(); api.read(); }",
        rooted_read_rule(),
        1,
    );
}

/// When no abrupt exit removes the conflicting path, the finding is
/// Possible because at least one reaching path matches.
#[test]
fn no_abrupt_exit_produces_possible_finding() {
    assert_count(
        "function run(flag) { let api = host.files; if (flag) api = local.files; api.read(); }",
        rooted_read_rule(),
        1,
    );
}

#[test]
fn exceptional_edges_join_try_and_catch_assignments() {
    let rule = rooted_read_rule();
    // Possible: host.files on the try path, local on the catch path.
    assert_count(
        "let api = local.files; try { api = host.files; } catch { api = api; } api.read();",
        rule.clone(),
        1,
    );
    // Definite: both try and catch have host.files.
    assert_count(
        "let api = host.files; try { api = host.files; } catch { api = host.files; } api.read();",
        rule.clone(),
        1,
    );
    // Possible: host.files on the catch path (unchanged), local on the try path.
    assert_count(
        "let api = host.files; try { api = local.files; } catch {} api.read();",
        rule,
        1,
    );
}

#[test]
fn direct_alias_reassignment_to_local_stays_local() {
    let rule = rule("fetch")
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    assert_count(
        "let reassignedFetch = fetch; reassignedFetch = localFetch; reassignedFetch('/local');",
        rule,
        0,
    );
}
