use super::*;

fn temp_root(_label: &str) -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn create(root: &Path, output: &Path) -> ProfileManifest {
    create_profile_manifest(root, &[], &[], None, 7, "fixture", output).unwrap()
}

#[test]
fn manifest_round_trip_verifies_digest_paths_bytes_and_hashes() {
    let root = temp_root("round-trip");
    fs::write(root.path().join("a.js"), "fetch('/a');").unwrap();
    fs::write(root.path().join("b.ts"), "const value: number = 1;").unwrap();
    let output = root.path().join("manifest.json");
    let manifest = create(root.path(), &output);
    let verified = verify_profile_manifest(root.path(), &output).unwrap();
    assert_eq!(
        verified.paths,
        vec![root.path().join("a.js"), root.path().join("b.ts")]
    );
    assert_eq!(verified.total_bytes, manifest.total_bytes());
    assert_eq!(verified.digest, manifest.digest());
}

#[test]
fn manifest_verification_rejects_missing_added_and_changed_selected_files() {
    let root = temp_root("content");
    fs::write(root.path().join("a.js"), "a();").unwrap();
    let output = root.path().join("manifest.json");
    create(root.path(), &output);

    fs::write(root.path().join("a.js"), "changed();").unwrap();
    assert!(
        verify_profile_manifest(root.path(), &output)
            .unwrap_err()
            .to_string()
            .contains("content mismatch")
    );
    fs::write(root.path().join("a.js"), "a();").unwrap();
    fs::write(root.path().join("added.js"), "").unwrap();
    assert!(
        verify_profile_manifest(root.path(), &output)
            .unwrap_err()
            .to_string()
            .contains("selected paths differ")
    );
    fs::remove_file(root.path().join("added.js")).unwrap();
    fs::remove_file(root.path().join("a.js")).unwrap();
    assert!(
        verify_profile_manifest(root.path(), &output)
            .unwrap_err()
            .to_string()
            .contains("selected paths differ")
    );
}

#[test]
fn manifest_paths_reject_duplicates_traversal_absolute_and_symlink_escape() {
    assert!(validate_relative("../escape.js").is_err());
    assert!(validate_relative("/absolute.js").is_err());

    let root = temp_root("paths");
    fs::write(root.path().join("a.js"), "").unwrap();
    let output = root.path().join("manifest.json");
    let mut manifest = create(root.path(), &output);
    manifest.body.files.push(manifest.body.files[0].clone());
    manifest.body.file_count += 1;
    manifest.manifest_digest = digest_json(&manifest.body).unwrap();
    fs::write(&output, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    assert!(
        verify_profile_manifest(root.path(), &output)
            .unwrap_err()
            .to_string()
            .contains("sorted and unique")
    );

    #[cfg(unix)]
    {
        let outside = temp_root("outside");
        fs::write(outside.path().join("outside.js"), "").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("outside.js"),
            root.path().join("link.js"),
        )
        .unwrap();
        assert!(
            manifest_entry(
                &fs::canonicalize(root.path()).unwrap(),
                &root.path().join("link.js"),
            )
            .unwrap_err()
            .to_string()
            .contains("escapes root")
        );
    }
}
