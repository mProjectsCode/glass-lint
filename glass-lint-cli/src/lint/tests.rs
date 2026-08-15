use std::fs;

use glass_lint_project::{ProjectSelection, SourceCollection, ValidatedProjectLoadOptions};

use super::*;

#[test]
fn directory_selection_prefers_local_tsconfig() {
    let root =
        std::env::temp_dir().join(format!("glass-lint-cli-selection-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();

    assert_eq!(
        project_selection(&root),
        ProjectSelection::Directory(root.clone())
    );

    let tsconfig = root.join("tsconfig.json");
    fs::write(&tsconfig, "{}").unwrap();
    assert_eq!(
        project_selection(&root),
        ProjectSelection::Tsconfig(tsconfig)
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discovers_sorted_runtime_javascript_and_typescript_files()
-> Result<(), Box<dyn std::error::Error>> {
    let root =
        std::env::temp_dir().join(format!("glass-lint-cli-discovery-{}", std::process::id()));
    fs::create_dir_all(&root)?;
    for filename in ["z.ts", "a.mjs", "c.d.ts", "b.cts", "ignored.txt"] {
        fs::write(root.join(filename), "")?;
    }

    let options = ValidatedProjectLoadOptions::default();
    let paths =
        SourceCollection::from_validated(&options)?.discover(std::slice::from_ref(&root))?;
    let names: Vec<_> = paths
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["a.mjs", "b.cts", "z.ts"]);

    fs::remove_dir_all(root)?;
    Ok(())
}
