use super::*;
#[test]
fn extends_within_budget_succeeds() {
    let project = TempProject::new("budget-within-extends");
    project.write("base.json", r#"{"include":["src/**/*"]}"#);
    project.write(
        "tsconfig.json",
        r#"{"extends":"./base.json","include":["lib/**/*"]}"#,
    );

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let budget = ConfigTraversalBudget::new(10, 5);
    let result = build_effective_config(
        &project.root().join("tsconfig.json"),
        project.root(),
        None,
        &mut diagnostics,
        budget,
        &mut config_count,
        &mut resource_budget,
    );

    assert!(result.is_ok(), "within-budget extends should succeed");
}

#[test]
fn extends_exceeding_max_depth_fails() {
    let project = TempProject::new("budget-depth-extends");
    // Chain: a -> b -> c with max_depth=2 should fail
    project.write("c.json", r#"{"include":["c/**/*"]}"#);
    project.write("b.json", r#"{"extends":"./c.json","include":["b/**/*"]}"#);
    project.write("a.json", r#"{"extends":"./b.json","include":["a/**/*"]}"#);

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    // max_depth=2 allows root + one extends but not root + two extends
    let budget = ConfigTraversalBudget::new(10, 2);
    let err = build_effective_config(
        &project.root().join("a.json"),
        project.root(),
        None,
        &mut diagnostics,
        budget,
        &mut config_count,
        &mut resource_budget,
    )
    .unwrap_err();

    assert!(
        matches!(
            err,
            ProjectLoadError::ConfigBudgetExhausted {
                kind: "extends depth",
                ..
            }
        ),
        "expected extends depth error, got {err:?}"
    );
}

#[test]
fn extends_exceeding_max_config_count_fails() {
    let project = TempProject::new("budget-count-extends");
    // Chain: a -> b -> c with max_config_count=2 should fail (3 configs)
    project.write("c.json", r#"{"include":["c/**/*"]}"#);
    project.write("b.json", r#"{"extends":"./c.json","include":["b/**/*"]}"#);
    project.write("a.json", r#"{"extends":"./b.json","include":["a/**/*"]}"#);

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let budget = ConfigTraversalBudget::new(2, 10);
    let err = build_effective_config(
        &project.root().join("a.json"),
        project.root(),
        None,
        &mut diagnostics,
        budget,
        &mut config_count,
        &mut resource_budget,
    )
    .unwrap_err();

    assert!(
        matches!(
            err,
            ProjectLoadError::ConfigBudgetExhausted {
                kind: "config count",
                ..
            }
        ),
        "expected config count error, got {err:?}"
    );
}

#[test]
fn extends_at_max_config_count_succeeds() {
    let project = TempProject::new("budget-count-at");
    // Chain: a -> b with max_config_count=2 should succeed (2 configs)
    project.write("b.json", r#"{"include":["b/**/*"]}"#);
    project.write("a.json", r#"{"extends":"./b.json","include":["a/**/*"]}"#);

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let budget = ConfigTraversalBudget::new(2, 10);
    let result = build_effective_config(
        &project.root().join("a.json"),
        project.root(),
        None,
        &mut diagnostics,
        budget,
        &mut config_count,
        &mut resource_budget,
    );

    assert!(result.is_ok(), "at-limit extends should succeed");
}

#[test]
fn extends_at_max_depth_succeeds() {
    let project = TempProject::new("budget-depth-at");
    // Chain: a -> b with max_depth=2 should succeed (depth: root=a, then b)
    project.write("b.json", r#"{"include":["b/**/*"]}"#);
    project.write("a.json", r#"{"extends":"./b.json","include":["a/**/*"]}"#);

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let budget = ConfigTraversalBudget::new(10, 2);
    let result = build_effective_config(
        &project.root().join("a.json"),
        project.root(),
        None,
        &mut diagnostics,
        budget,
        &mut config_count,
        &mut resource_budget,
    );

    assert!(result.is_ok(), "at-limit depth extends should succeed");
}

// ---------------------------------------------------------------------------
// Cross-directory extends — paths must be rebased to the declaring config's
// directory before merging.
// ---------------------------------------------------------------------------
