//! Bundler process boundary and the normalized transformation contract.
//!
//! The harness owns this boundary deliberately. JavaScript toolchains receive
//! only a validated, profile-shaped request and return one generated asset;
//! they never participate in rule selection or expectation matching.

use std::{
    fmt::Write as _,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::types::{AdapterFile, BundleProfile, BundleTarget, BundleTransformer, Case};

pub const BUNDLER_PROTOCOL_VERSION: u32 = 1;
pub const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_FILES: usize = 256;
pub const MAX_FILE_BYTES: usize = 512 * 1024;
pub const MAX_GENERATED_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_STDERR_BYTES: usize = 8 * 1024;
pub const MAX_STDOUT_BYTES: usize = MAX_GENERATED_BYTES + 64 * 1024;
const PROCESS_LIMIT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct BundleRequest {
    pub protocol_version: u32,
    pub transformer: BundleTransformer,
    pub profile: BundleProfile,
    pub entry: String,
    pub language: String,
    pub minified: bool,
    pub target: BundleTarget,
    pub files: Vec<AdapterFile>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct BundleResponse {
    pub protocol_version: u32,
    pub transformer: BundleTransformer,
    pub transformer_version: String,
    pub profile: BundleProfile,
    pub generated_source: String,
}

#[derive(Clone, Debug)]
pub struct BundleOutput {
    pub transformer_version: String,
    pub source: String,
    pub bytes: usize,
    pub digest: String,
}

pub trait Bundler {
    fn bundle(&self, request: &BundleRequest) -> Result<BundleOutput>;
}

#[derive(Clone, Debug)]
pub struct ProcessBundler {
    pub command: PathBuf,
    pub runner: PathBuf,
}

impl Default for ProcessBundler {
    fn default() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("harness has a workspace parent")
            .to_owned();
        Self {
            command: PathBuf::from("bun"),
            runner: root.join("tools/bundlers/runner.ts"),
        }
    }
}

impl Bundler for ProcessBundler {
    fn bundle(&self, request: &BundleRequest) -> Result<BundleOutput> {
        validate_request(request)?;
        let encoded = serde_json::to_vec(request)?;
        if encoded.len() > MAX_REQUEST_BYTES {
            bail!("bundle request exceeds {MAX_REQUEST_BYTES} bytes");
        }
        let runner_dir = self
            .runner
            .parent()
            .context("bundler runner has no parent directory")?;
        let mut child = Command::new(&self.command)
            .arg("run")
            .arg(&self.runner)
            .current_dir(runner_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("start bundler {}", self.command.display()))?;
        child
            .stdin
            .as_mut()
            .context("bundler stdin unavailable")?
            .write_all(&encoded)?;
        child.stdin.take();
        let stdout = child.stdout.take().context("bundler stdout unavailable")?;
        let stderr = child.stderr.take().context("bundler stderr unavailable")?;
        let stdout_reader = thread::spawn(move || read_bounded(stdout, MAX_STDOUT_BYTES));
        let stderr_reader = thread::spawn(move || read_bounded(stderr, MAX_STDERR_BYTES));

        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                let stdout = stdout_reader
                    .join()
                    .map_err(|_| anyhow::anyhow!("bundler stdout reader panicked"))??;
                let stderr = stderr_reader
                    .join()
                    .map_err(|_| anyhow::anyhow!("bundler stderr reader panicked"))??;
                if stdout.len() > MAX_STDOUT_BYTES {
                    bail!("bundler response exceeds {MAX_STDOUT_BYTES} bytes");
                }
                if !status.success() {
                    bail!(
                        "bundler exited {status}: {}",
                        bounded_text(&stderr, MAX_STDERR_BYTES)
                    );
                }
                return decode_response(&stdout, request);
            }
            if start.elapsed() >= PROCESS_LIMIT {
                child.kill().ok();
                child.wait().ok();
                bail!(
                    "bundler exceeded {} second process limit",
                    PROCESS_LIMIT.as_secs()
                );
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn read_bounded(mut reader: impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn validate_request(request: &BundleRequest) -> Result<()> {
    if request.protocol_version != BUNDLER_PROTOCOL_VERSION {
        bail!(
            "bundle protocol version {}, expected {}",
            request.protocol_version,
            BUNDLER_PROTOCOL_VERSION
        );
    }
    if request.files.is_empty() || request.files.len() > MAX_FILES {
        bail!("bundle request must contain 1..={MAX_FILES} files");
    }
    if !request.files.iter().any(|file| file.path == request.entry) {
        bail!(
            "bundle entry '{}' is not present in the supplied files",
            request.entry
        );
    }
    for file in &request.files {
        if file.source.len() > MAX_FILE_BYTES {
            bail!(
                "bundle input '{}' exceeds {} bytes",
                file.path,
                MAX_FILE_BYTES
            );
        }
    }
    Ok(())
}

fn decode_response(bytes: &[u8], request: &BundleRequest) -> Result<BundleOutput> {
    let response: BundleResponse = serde_json::from_slice(bytes)
        .map_err(|error| anyhow::anyhow!("invalid bundler response: {error}"))?;
    if response.protocol_version != BUNDLER_PROTOCOL_VERSION {
        bail!(
            "bundler protocol version {}, expected {}",
            response.protocol_version,
            BUNDLER_PROTOCOL_VERSION
        );
    }
    if response.transformer != request.transformer || response.profile != request.profile {
        bail!("bundler response identity does not match request");
    }
    if response.generated_source.len() > MAX_GENERATED_BYTES {
        bail!("generated bundle exceeds {MAX_GENERATED_BYTES} bytes");
    }
    let bytes = response.generated_source.len();
    let digest = digest(&response.generated_source);
    Ok(BundleOutput {
        transformer_version: response.transformer_version,
        source: response.generated_source,
        bytes,
        digest,
    })
}

fn bounded_text(bytes: &[u8], limit: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= limit {
        text.into_owned()
    } else {
        format!(
            "{}… [truncated]",
            text.chars().take(limit).collect::<String>()
        )
    }
}

#[must_use]
pub fn digest(source: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    let mut output = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

#[must_use]
pub fn request_for_case(
    case: &Case,
    profile: BundleProfile,
    transformer: BundleTransformer,
    minified: bool,
    target: BundleTarget,
) -> BundleRequest {
    let (entry, files) = case.project.as_ref().map_or_else(
        || {
            (
                case.filename.clone(),
                vec![AdapterFile {
                    path: case.filename.clone(),
                    language: case.language.clone(),
                    source: case.source.clone(),
                }],
            )
        },
        |project| {
            (
                project.protocol.entries[0].clone(),
                project.protocol.files.clone(),
            )
        },
    );
    BundleRequest {
        protocol_version: BUNDLER_PROTOCOL_VERSION,
        transformer,
        profile,
        entry,
        language: case.language.clone(),
        minified,
        target,
        files,
    }
}

#[cfg(test)]
mod tests;
