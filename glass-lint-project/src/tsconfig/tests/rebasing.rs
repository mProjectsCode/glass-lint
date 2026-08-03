use super::*;
#[test]
fn cross_dir_include_is_rebased_to_parent_dir() {
    // Parent at sub/tsconfig.json declares include: ["src"], which means
    // sub/src relative to the project root.  Child at tsconfig.json extends
    // parent; the inherited pattern must resolve to sub/src, not src.
    let project = TempProject::new("cross-include");
    project.create_dir("sub/src");
    project.write("sub/tsconfig.json", r#"{"include":["src"]}"#);
    project.write("tsconfig.json", r#"{"extends":"./sub/tsconfig.json"}"#);

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let (config, _) = build_effective_config(
        &project.root().join("tsconfig.json"),
        project.root(),
        None,
        &mut diagnostics,
        default_budget(),
        &mut config_count,
        &mut resource_budget,
    )
    .unwrap();

    // Parent's "src" was rebased to "sub/src"; the pattern becomes
    // "sub/src" which the glob matcher treats as an exact directory
    // reference.  In TypeScript, include:["src"] would be expanded to
    // "src/**/*", but the current normalize only expands trailing-slash
    // patterns, so we assert the exact path matches.
    assert!(
        config.includes(Path::new("sub/src")),
        "rebased parent include should match sub/src"
    );
    assert!(
        !config.includes(Path::new("src")),
        "unrebased parent include should NOT match root src"
    );
}

#[test]
fn cross_dir_files_is_rebased_to_parent_dir() {
    // Parent declares an explicit files entry; the path is relative to the
    // parent's directory and must be rebased for the child.
    let project = TempProject::new("cross-files");
    project.create_dir("sub/src");
    project.write("sub/tsconfig.json", r#"{"files":["src/file.ts"]}"#);
    project.write("sub/src/file.ts", "");
    project.write("tsconfig.json", r#"{"extends":"./sub/tsconfig.json"}"#);

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let (config, _) = build_effective_config(
        &project.root().join("tsconfig.json"),
        project.root(),
        None,
        &mut diagnostics,
        default_budget(),
        &mut config_count,
        &mut resource_budget,
    )
    .unwrap();

    let files = config.explicit_files().expect("files should be Some");
    assert_eq!(files, vec!["sub/src/file.ts"]);
}

#[test]
fn cross_dir_exclude_is_rebased_to_parent_dir() {
    // Parent excludes a path relative to its own directory.  The child
    // inherits the exclude without setting its own, so the rebased path
    // must be used.
    let project = TempProject::new("cross-exclude");
    project.create_dir("sub");
    project.write(
        "sub/tsconfig.json",
        r#"{"include":["**/*"],"exclude":["secret.ts"]}"#,
    );
    project.write(
        "tsconfig.json",
        r#"{"extends":"./sub/tsconfig.json","include":["**/*"]}"#,
    );

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let (config, _) = build_effective_config(
        &project.root().join("tsconfig.json"),
        project.root(),
        None,
        &mut diagnostics,
        default_budget(),
        &mut config_count,
        &mut resource_budget,
    )
    .unwrap();

    // Parent's "secret.ts" rebased to "sub/secret.ts"
    assert!(
        config.includes(Path::new("main.ts")),
        "unrelated file should be included"
    );
    assert!(
        !config.includes(Path::new("sub/secret.ts")),
        "rebased parent exclude should apply to sub/secret.ts"
    );
    assert!(
        config.includes(Path::new("secret.ts")),
        "parent's exclude should NOT apply to root/secret.ts"
    );
}

#[test]
fn cross_dir_outdir_in_exclude_is_rebased() {
    // Parent has outDir set, which is automatically added to the exclude
    // list.  The child inherits the parent's exclude and the outDir path
    // must be rebased.
    let project = TempProject::new("cross-outdir");
    project.create_dir("sub");
    project.write(
        "sub/tsconfig.json",
        r#"{"compilerOptions":{"outDir":"out"},"include":["**/*"]}"#,
    );
    project.write(
        "tsconfig.json",
        r#"{"extends":"./sub/tsconfig.json","include":["**/*"]}"#,
    );

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let (config, _) = build_effective_config(
        &project.root().join("tsconfig.json"),
        project.root(),
        None,
        &mut diagnostics,
        default_budget(),
        &mut config_count,
        &mut resource_budget,
    )
    .unwrap();

    // Parent's "out" was rebased: "sub/out" should be in the exclude list.
    // The current glob matching for non-trailing-slash excludes only matches
    // the exact path (not children), so we test the directory path itself.
    assert!(
        !config.includes(Path::new("sub/out")),
        "rebased parent outDir should exclude sub/out"
    );
    // Root's out is NOT excluded by the parent's setting
    assert!(
        config.includes(Path::new("out")),
        "parent's outDir should NOT exclude root/out"
    );
}

#[test]
fn cross_dir_child_include_overrides_parent() {
    // Child sets its own include, overriding the parent's include.
    // The parent's include must NOT leak into the child's selection.
    let project = TempProject::new("cross-override");
    project.create_dir("sub");
    project.write("sub/tsconfig.json", r#"{"include":["sub_only/**/*"]}"#);
    project.write(
        "tsconfig.json",
        r#"{"extends":"./sub/tsconfig.json","include":["child_only/**/*"]}"#,
    );

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let (config, _) = build_effective_config(
        &project.root().join("tsconfig.json"),
        project.root(),
        None,
        &mut diagnostics,
        default_budget(),
        &mut config_count,
        &mut resource_budget,
    )
    .unwrap();

    // Only child's include should be active
    assert!(
        config.includes(Path::new("child_only/main.ts")),
        "child include should be used"
    );
    assert!(
        !config.includes(Path::new("sub_only/main.ts")),
        "parent include should NOT leak when child overrides"
    );
}

#[test]
fn cross_dir_child_exclude_inherits_rebased_parent_exclude() {
    // Child does NOT set its own exclude, so it inherits the parent's
    // exclude list (rebased to the child's directory).
    let project = TempProject::new("cross-exclude-inherit");
    project.create_dir("sub");
    project.write(
        "sub/tsconfig.json",
        r#"{"include":["**/*"],"exclude":["sub_secret.ts"]}"#,
    );
    project.write(
        "tsconfig.json",
        r#"{"extends":"./sub/tsconfig.json","include":["**/*"]}"#,
    );

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let (config, _) = build_effective_config(
        &project.root().join("tsconfig.json"),
        project.root(),
        None,
        &mut diagnostics,
        default_budget(),
        &mut config_count,
        &mut resource_budget,
    )
    .unwrap();

    // Parent's "sub_secret.ts" rebased to "sub/sub_secret.ts"
    assert!(
        !config.includes(Path::new("sub/sub_secret.ts")),
        "rebased parent exclude should apply to sub/sub_secret.ts"
    );
    assert!(
        config.includes(Path::new("sub_secret.ts")),
        "parent exclude should NOT apply to root/sub_secret.ts"
    );
}

#[test]
fn cross_dir_child_exclude_overrides_parent() {
    // Child sets its own exclude, overriding the parent's exclude entirely.
    // The parent's exclude must NOT be inherited.
    let project = TempProject::new("cross-exclude-override");
    project.create_dir("sub");
    project.write(
        "sub/tsconfig.json",
        r#"{"include":["**/*"],"exclude":["sub_secret.ts"]}"#,
    );
    project.write(
        "tsconfig.json",
        r#"{"extends":"./sub/tsconfig.json","include":["**/*"],"exclude":["child_secret.ts"]}"#,
    );

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let (config, _) = build_effective_config(
        &project.root().join("tsconfig.json"),
        project.root(),
        None,
        &mut diagnostics,
        default_budget(),
        &mut config_count,
        &mut resource_budget,
    )
    .unwrap();

    // Child's exclude overrides parent's
    assert!(
        config.includes(Path::new("sub/sub_secret.ts")),
        "parent exclude should NOT apply when child overrides"
    );
    assert!(
        !config.includes(Path::new("child_secret.ts")),
        "child exclude should apply"
    );
}

#[test]
fn cross_dir_three_level_chain_rebases_correctly() {
    // grandparent/tsconfig.json  include:["src/**/*"]
    //   parent/tsconfig.json     extends ../grandparent/tsconfig.json
    //   child/tsconfig.json      extends ../parent/tsconfig.json
    //
    // The grandparent's "src/**/*" should be rebased twice:
    //   grandparent->parent: "../grandparent/src/**/*"
    //   parent->child:       "../grandparent/src/**/*"
    let project = TempProject::new("cross-three-level");
    project.create_dir("grandparent");
    project.create_dir("parent");
    project.create_dir("child");
    project.write("grandparent/tsconfig.json", r#"{"include":["src/**/*"]}"#);
    project.write(
        "parent/tsconfig.json",
        r#"{"extends":"../grandparent/tsconfig.json"}"#,
    );
    project.write(
        "child/tsconfig.json",
        r#"{"extends":"../parent/tsconfig.json"}"#,
    );

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let (config, _) = build_effective_config(
        &project.root().join("child/tsconfig.json"),
        project.root(),
        None,
        &mut diagnostics,
        default_budget(),
        &mut config_count,
        &mut resource_budget,
    )
    .unwrap();

    // The grandparent's "src/**/*" rebases to "../grandparent/src/**/*"
    // relative to the child config.  The path "../grandparent/src/main.ts"
    // should match.
    // The grandparent's "src/**/*" rebases to "../grandparent/src/**/*"
    // relative to the child config.  The path "../grandparent/src/main.ts"
    // should match.
    assert!(
        config.includes(Path::new("../grandparent/src/main.ts")),
        "grandparent include should match grandparent/src after two rebases"
    );
    // "src/main.ts" relative to child means child/src/main.ts — should NOT match.
    assert!(
        !config.includes(Path::new("src/main.ts")),
        "grandparent include should not match child/src"
    );
}
