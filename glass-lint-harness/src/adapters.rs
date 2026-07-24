//! Built-in and external adapter boundaries for conformance execution.
//!
//! Adapters normalize tool-specific execution into findings plus optional
//! file-qualified locations; the runner can then compare every tool uniformly.

use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use glass_lint_core::{
    Linter, LinterConfig, RuleBaseline, RuleOverride, RuleSelection, RuleState,
    project::{Finding, SourceFile},
};

use crate::types::{
    ADAPTER_PROTOCOL_VERSION, AdapterProject, AdapterRequest, AdapterResolution, AdapterResponse,
    AdapterRun, Case, ProjectCase, ToolExpectation,
};

pub trait Adapter {
    /// Stable name used to select expectations and label report columns.
    fn name(&self) -> &str;
    /// Return the adapter version without running a case.
    fn version(&self) -> Result<String>;
    /// Execute one case and return its normalized findings.
    fn run(&self, case: &Case, expectation: &ToolExpectation) -> Result<Vec<Finding>>;

    /// Execute one case while retaining canonical source locations.
    fn run_with_locations(&self, case: &Case, expectation: &ToolExpectation) -> Result<AdapterRun> {
        let findings = self.run(case, expectation)?;
        Ok(AdapterRun { findings })
    }

    /// Whether the adapter accepts multi-file project requests.
    fn supports_projects(&self) -> bool {
        false
    }
}

pub struct GlassLintAdapter;

impl Adapter for GlassLintAdapter {
    fn name(&self) -> &'static str {
        "glass-lint"
    }

    fn version(&self) -> Result<String> {
        Ok(env!("CARGO_PKG_VERSION").into())
    }

    fn supports_projects(&self) -> bool {
        true
    }

    fn run_with_locations(&self, case: &Case, expectation: &ToolExpectation) -> Result<AdapterRun> {
        if let Some(project) = &case.project {
            return run_project(project, expectation);
        }
        let findings = self.run(case, expectation)?;
        Ok(AdapterRun { findings })
    }

    fn run(&self, case: &Case, expectation: &ToolExpectation) -> Result<Vec<Finding>> {
        if let Some(project) = &case.project {
            return Ok(run_project(project, expectation)?.findings);
        }
        Ok(project_report_to_run(
            &configured_linter(expectation)?.lint_snippet(&case.source, &case.filename)?,
        )?
        .findings)
    }
}

fn configured_linter(expectation: &ToolExpectation) -> Result<Linter> {
    let environment = glass_lint_obsidian::obsidian_environment();
    let catalogs = vec![
        glass_lint_js::js_catalog(),
        glass_lint_js::browser_catalog(),
        glass_lint_js::node_catalog(),
        glass_lint_js::electron_catalog(),
        glass_lint_obsidian::obsidian_catalog(),
    ];
    if let Some(config) = expectation.config() {
        if config != "heuristic" {
            bail!("unknown built-in glass-lint config `{config}`");
        }
        return Ok(Linter::new(LinterConfig::new(catalogs, environment))?);
    }
    let enabled = expectation
        .rules()
        .iter()
        .map(|id| glass_lint_core::RuleId::parse(id.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    if enabled.is_empty() {
        bail!("project tool has no selected built-in rules");
    }
    let selection =
        enabled
            .into_iter()
            .try_fold(RuleSelection::new(RuleBaseline::None), |selection, id| {
                Ok::<_, glass_lint_core::LintConfigError>(
                    selection.with_override(RuleOverride::new(id.to_string(), RuleState::Enabled)?),
                )
            })?;
    Ok(Linter::new(
        LinterConfig::new(catalogs, environment).with_rules(selection),
    )?)
}

fn run_project(project: &ProjectCase, expectation: &ToolExpectation) -> Result<AdapterRun> {
    // Filesystem projects use the project loader; virtual projects use the
    // session API, but both paths converge on the same report conversion.
    let linter = configured_linter(expectation)?;
    let report = if project.filesystem {
        glass_lint_project::ProjectLoader::new(
            glass_lint_project::ValidatedProjectLoadOptions::default(),
        )
        .load_and_lint(
            &linter,
            &glass_lint_project::ProjectSelection::directory(project.root()),
        )?
        .report
    } else {
        let mut session = linter.begin_project(project.root())?;
        let mut authored = Vec::new();
        let mut outcomes = Vec::new();
        for file in project.files() {
            authored.extend(
                session
                    .analyze_source(SourceFile::new(file.path.clone(), file.source.clone())?)?
                    .requests()
                    .into_iter()
                    .map(|request| (request, file.path.clone())),
            );
        }
        for resolution in project.resolutions() {
            let (kind, result) = <&AdapterResolution as TryInto<(_, _)>>::try_into(resolution)
                .map_err(|error: String| anyhow::anyhow!(error))?;
            let request = authored
                .iter()
                .find(|(candidate, importer)| {
                    importer == &resolution.importer
                        && candidate.key.kind == kind
                        && candidate.key.range == resolution.range
                        && candidate.request == resolution.request
                })
                .map(|(request, _)| request.key.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "project resolution does not match {} {}",
                        resolution.importer,
                        resolution.request
                    )
                })?;
            outcomes.push((request, result));
        }
        session.finish_local().resolve(outcomes)?.finish()?
    };
    project_report_to_run(&report)
}

fn project_report_to_run(report: &glass_lint_core::project::AnalysisReport) -> Result<AdapterRun> {
    // Keep diagnostics fatal for harness execution: a partial project report
    // cannot be compared reliably against finding expectations.
    let diagnostics = report
        .files()
        .iter()
        .flat_map(|file| file.diagnostics().iter())
        .map(|diagnostic| diagnostic.message().to_owned())
        .chain(
            report
                .diagnostics()
                .iter()
                .map(|diagnostic| format!("[{}] {}", diagnostic.code(), diagnostic.message())),
        )
        .collect::<Vec<_>>();
    if !diagnostics.is_empty() {
        bail!("{}", diagnostics.join("; "));
    }
    let mut findings = Vec::new();
    for file in report.files() {
        findings.extend(file.findings().iter().cloned());
    }
    Ok(AdapterRun { findings })
}

pub struct ExternalAdapter {
    /// Name used by case tool blocks and report columns.
    pub name: String,
    /// Executable or script command implementing the adapter protocol.
    pub command: PathBuf,
}

impl Adapter for ExternalAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> Result<String> {
        Ok("external".into())
    }

    fn supports_projects(&self) -> bool {
        true
    }

    fn run_with_locations(&self, case: &Case, expectation: &ToolExpectation) -> Result<AdapterRun> {
        Ok(AdapterRun {
            findings: self.run_protocol(case, expectation)?,
        })
    }

    fn run(&self, case: &Case, expectation: &ToolExpectation) -> Result<Vec<Finding>> {
        self.run_protocol(case, expectation)
    }
}

impl ExternalAdapter {
    /// Send one complete JSON request and validate the complete response
    /// envelope.
    fn run_protocol(&self, case: &Case, expectation: &ToolExpectation) -> Result<Vec<Finding>> {
        let mut child = Command::new(&self.command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("start adapter {}", self.command.display()))?;
        let request = AdapterRequest {
            protocol_version: ADAPTER_PROTOCOL_VERSION,
            case_id: case.id.clone(),
            filename: case.filename.clone(),
            language: case.language.clone(),
            source: case.source.clone(),
            rules: expectation.rules().to_vec(),
            config: expectation.config().map(str::to_owned),
            project: case.project.as_ref().map(adapter_project),
        };
        serde_json::to_writer(
            child.stdin.as_mut().context("adapter stdin unavailable")?,
            &request,
        )?;
        child.stdin.take().unwrap().flush()?;
        let output = child.wait_with_output()?;
        if !output.status.success() {
            const STDERR_LIMIT: usize = 8 * 1024;
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = if stderr.len() > STDERR_LIMIT {
                format!(
                    "{}… [stderr truncated]",
                    stderr.chars().take(STDERR_LIMIT).collect::<String>()
                )
            } else {
                stderr.into_owned()
            };
            bail!("adapter exited {}: {}", output.status, stderr);
        }
        let response: AdapterResponse = serde_json::from_slice(&output.stdout)
            .map_err(|error| anyhow::anyhow!("invalid adapter response: {error}"))?;
        if response.protocol_version != ADAPTER_PROTOCOL_VERSION {
            bail!(
                "adapter protocol version {}, expected {}",
                response.protocol_version,
                ADAPTER_PROTOCOL_VERSION
            );
        }
        if response.tool != self.name {
            bail!(
                "adapter identified as `{}`, expected `{}`",
                response.tool,
                self.name
            );
        }
        Ok(response.findings)
    }
}

fn adapter_project(project: &ProjectCase) -> AdapterProject {
    project.into()
}
