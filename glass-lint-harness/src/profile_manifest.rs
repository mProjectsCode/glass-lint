//! Immutable identity for a deterministically selected profiling corpus.

use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::profile::{discover_profile_files, sample_paths};

pub const PROFILE_MANIFEST_VERSION: u32 = 1;
pub const PROFILE_SELECTION_ALGORITHM_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileManifestEntry {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProfileManifestBody {
    manifest_version: u32,
    selection_algorithm_version: u32,
    seed: u64,
    requested_sample_size: Option<usize>,
    include: Vec<String>,
    exclude: Vec<String>,
    root_label: String,
    files: Vec<ProfileManifestEntry>,
    file_count: usize,
    total_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileManifest {
    #[serde(flatten)]
    body: ProfileManifestBody,
    manifest_digest: String,
}

impl ProfileManifest {
    pub fn digest(&self) -> &str {
        &self.manifest_digest
    }

    pub fn file_count(&self) -> usize {
        self.body.file_count
    }

    pub fn total_bytes(&self) -> u64 {
        self.body.total_bytes
    }
}

#[derive(Clone, Debug)]
pub struct VerifiedProfileManifest {
    pub paths: Vec<PathBuf>,
    pub total_bytes: u64,
    pub digest: String,
}

pub fn create_profile_manifest(
    root: &Path,
    include: &[String],
    exclude: &[String],
    sample: Option<usize>,
    seed: u64,
    root_label: &str,
    output: &Path,
) -> Result<ProfileManifest> {
    validate_root_label(root_label)?;
    let root = canonical_root(root)?;
    let mut paths = discover_profile_files(std::slice::from_ref(&root), include, exclude)?;
    if let Some(sample) = sample {
        sample_paths(&mut paths, sample, seed);
    }
    let files = paths
        .iter()
        .map(|path| manifest_entry(&root, path))
        .collect::<Result<Vec<_>>>()?;
    let total_bytes = files.iter().map(|entry| entry.bytes).sum();
    let body = ProfileManifestBody {
        manifest_version: PROFILE_MANIFEST_VERSION,
        selection_algorithm_version: PROFILE_SELECTION_ALGORITHM_VERSION,
        seed,
        requested_sample_size: sample,
        include: include.to_vec(),
        exclude: exclude.to_vec(),
        root_label: root_label.to_owned(),
        file_count: files.len(),
        total_bytes,
        files,
    };
    let manifest = ProfileManifest {
        manifest_digest: digest_json(&body)?,
        body,
    };
    let encoded = serde_json::to_vec_pretty(&manifest)?;
    fs::write(output, encoded).with_context(|| format!("write {}", output.display()))?;
    Ok(manifest)
}

pub fn verify_profile_manifest(
    root: &Path,
    manifest_path: &Path,
) -> Result<VerifiedProfileManifest> {
    let root = canonical_root(root)?;
    let encoded =
        fs::read(manifest_path).with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest: ProfileManifest = serde_json::from_slice(&encoded)
        .with_context(|| format!("parse {}", manifest_path.display()))?;
    if manifest.body.manifest_version != PROFILE_MANIFEST_VERSION {
        bail!(
            "unsupported profile manifest version {}",
            manifest.body.manifest_version
        );
    }
    if manifest.body.selection_algorithm_version != PROFILE_SELECTION_ALGORITHM_VERSION {
        bail!(
            "unsupported profile selection algorithm version {}",
            manifest.body.selection_algorithm_version
        );
    }
    validate_root_label(&manifest.body.root_label)?;
    let actual_digest = digest_json(&manifest.body)?;
    if actual_digest != manifest.manifest_digest {
        bail!("profile manifest digest mismatch");
    }
    if manifest.body.file_count != manifest.body.files.len() {
        bail!("profile manifest file count mismatch");
    }
    let declared_bytes = manifest
        .body
        .files
        .iter()
        .map(|entry| entry.bytes)
        .sum::<u64>();
    if declared_bytes != manifest.body.total_bytes {
        bail!("profile manifest byte total mismatch");
    }

    let mut discovered = discover_profile_files(
        std::slice::from_ref(&root),
        &manifest.body.include,
        &manifest.body.exclude,
    )?;
    if let Some(sample) = manifest.body.requested_sample_size {
        sample_paths(&mut discovered, sample, manifest.body.seed);
    }
    let expected = discovered
        .iter()
        .map(|path| normalized_relative(&root, path))
        .collect::<Result<Vec<_>>>()?;
    let declared = manifest
        .body
        .files
        .iter()
        .map(|entry| validate_relative(&entry.path))
        .collect::<Result<Vec<_>>>()?;
    if declared.windows(2).any(|pair| pair[0] >= pair[1]) {
        bail!("profile manifest paths must be sorted and unique");
    }
    if expected != declared {
        bail!("profile manifest selected paths differ from the current corpus");
    }

    let mut paths = Vec::with_capacity(manifest.body.files.len());
    let mut seen = BTreeSet::new();
    for (entry, relative) in manifest.body.files.iter().zip(declared) {
        if !seen.insert(relative.clone()) {
            bail!("duplicate profile manifest path `{relative}`");
        }
        let path = root.join(&relative);
        let actual = manifest_entry(&root, &path)?;
        if actual.bytes != entry.bytes || actual.sha256 != entry.sha256 {
            bail!("profile manifest content mismatch for `{relative}`");
        }
        paths.push(path);
    }
    Ok(VerifiedProfileManifest {
        paths,
        total_bytes: manifest.body.total_bytes,
        digest: manifest.manifest_digest,
    })
}

fn canonical_root(root: &Path) -> Result<PathBuf> {
    let root =
        fs::canonicalize(root).with_context(|| format!("canonicalize {}", root.display()))?;
    if !root.is_dir() {
        bail!("profile manifest root must be a directory");
    }
    Ok(root)
}

fn manifest_entry(root: &Path, path: &Path) -> Result<ProfileManifestEntry> {
    let relative = normalized_relative(root, path)?;
    let canonical =
        fs::canonicalize(path).with_context(|| format!("canonicalize {}", path.display()))?;
    if !canonical.starts_with(root) {
        bail!("profile path escapes root: {}", path.display());
    }
    let bytes = fs::read(&canonical).with_context(|| format!("read {}", path.display()))?;
    Ok(ProfileManifestEntry {
        path: relative,
        bytes: u64::try_from(bytes.len()).context("profile file length exceeds u64")?,
        sha256: digest_bytes(&bytes),
    })
}

fn normalized_relative(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("profile path is outside root: {}", path.display()))?;
    validate_relative(&relative.to_string_lossy().replace('\\', "/"))
}

fn validate_relative(path: &str) -> Result<String> {
    let candidate = Path::new(path);
    if path.is_empty()
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("invalid profile manifest path `{path}`");
    }
    Ok(path.replace('\\', "/"))
}

fn validate_root_label(label: &str) -> Result<()> {
    if label.trim().is_empty() || label.contains(['/', '\\', '\0']) {
        bail!("profile manifest root label must be a nonempty label");
    }
    Ok(())
}

fn digest_json(value: &impl Serialize) -> Result<String> {
    Ok(digest_bytes(&serde_json::to_vec(value)?))
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:?}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(_label: &str) -> crate::test_support::TempDir {
        crate::test_support::TempDir::new()
    }

    fn create(root: &Path, output: &Path) -> ProfileManifest {
        create_profile_manifest(root, &[], &[], None, 7, "fixture", output).unwrap()
    }

    #[test]
    fn manifest_round_trip_verifies_digest_paths_bytes_and_hashes() {
        let root = temp_root("round-trip");
        fs::write(root.join("a.js"), "fetch('/a');").unwrap();
        fs::write(root.join("b.ts"), "const value: number = 1;").unwrap();
        let output = root.join("manifest.json");
        let manifest = create(&root, &output);
        let verified = verify_profile_manifest(&root, &output).unwrap();
        assert_eq!(verified.paths, vec![root.join("a.js"), root.join("b.ts")]);
        assert_eq!(verified.total_bytes, manifest.total_bytes());
        assert_eq!(verified.digest, manifest.digest());
    }

    #[test]
    fn manifest_verification_rejects_missing_added_and_changed_selected_files() {
        let root = temp_root("content");
        fs::write(root.join("a.js"), "a();").unwrap();
        let output = root.join("manifest.json");
        create(&root, &output);

        fs::write(root.join("a.js"), "changed();").unwrap();
        assert!(
            verify_profile_manifest(&root, &output)
                .unwrap_err()
                .to_string()
                .contains("content mismatch")
        );
        fs::write(root.join("a.js"), "a();").unwrap();
        fs::write(root.join("added.js"), "").unwrap();
        assert!(
            verify_profile_manifest(&root, &output)
                .unwrap_err()
                .to_string()
                .contains("selected paths differ")
        );
        fs::remove_file(root.join("added.js")).unwrap();
        fs::remove_file(root.join("a.js")).unwrap();
        assert!(
            verify_profile_manifest(&root, &output)
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
        fs::write(root.join("a.js"), "").unwrap();
        let output = root.join("manifest.json");
        let mut manifest = create(&root, &output);
        manifest.body.files.push(manifest.body.files[0].clone());
        manifest.body.file_count += 1;
        manifest.manifest_digest = digest_json(&manifest.body).unwrap();
        fs::write(&output, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
        assert!(
            verify_profile_manifest(&root, &output)
                .unwrap_err()
                .to_string()
                .contains("sorted and unique")
        );

        #[cfg(unix)]
        {
            let outside = temp_root("outside");
            fs::write(outside.join("outside.js"), "").unwrap();
            std::os::unix::fs::symlink(outside.join("outside.js"), root.join("link.js")).unwrap();
            assert!(
                manifest_entry(&fs::canonicalize(&root).unwrap(), &root.join("link.js"))
                    .unwrap_err()
                    .to_string()
                    .contains("escapes root")
            );
        }
    }
}
