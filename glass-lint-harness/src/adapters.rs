use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use glass_lint_core::{Finding, Linter, SourceFile};

use crate::{
    builtins::{self, BuiltInProfile},
    types::{
        ADAPTER_PROTOCOL_VERSION, AdapterProject, AdapterRequest, AdapterResolution,
        AdapterResponse, AdapterRun, Case, FindingLocation, ProjectCase, ToolExpectation,
    },
};

pub trait Adapter {
    fn name(&self) -> &str;
    fn version(&self) -> Result<String>;
    fn run(&self, case: &Case, expectation: &ToolExpectation) -> Result<Vec<Finding>>;

    fn run_with_locations(&self, case: &Case, expectation: &ToolExpectation) -> Result<AdapterRun> {
        let findings = self.run(case, expectation)?;
        let finding_locations = findings
            .iter()
            .map(|_| FindingLocation {
                primary: None,
                evidence: Vec::new(),
            })
            .collect();
        Ok(AdapterRun {
            findings,
            finding_locations,
        })
    }

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
        Ok(AdapterRun {
            finding_locations: findings
                .iter()
                .map(|finding| FindingLocation {
                    primary: Some(case.filename.clone()),
                    evidence: finding
                        .evidence
                        .iter()
                        .map(|_| Some(case.filename.clone()))
                        .collect(),
                })
                .collect(),
            findings,
        })
    }

    fn run(&self, case: &Case, expectation: &ToolExpectation) -> Result<Vec<Finding>> {
        if let Some(project) = &case.project {
            return Ok(run_project(project, expectation)?.findings);
        }
        let report = configured_linter(expectation)?.lint(&case.source, &case.filename);
        if !report.parse_diagnostics.is_empty() {
            bail!(
                "{}",
                report
                    .parse_diagnostics
                    .into_iter()
                    .map(|d| d.message)
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
        Ok(report.findings)
    }
}

fn configured_linter(expectation: &ToolExpectation) -> Result<Linter> {
    let environment = glass_lint_obsidian::default_environment();
    let js = builtins::linter(
        builtins::BuiltInProvider::Js,
        BuiltInProfile::Heuristic,
        environment.clone(),
    );
    let obsidian = builtins::linter(
        builtins::BuiltInProvider::Obsidian,
        BuiltInProfile::Heuristic,
        environment.clone(),
    );
    if let Some(config) = expectation.config.as_deref() {
        if config != "heuristic" {
            bail!("unknown built-in glass-lint config `{config}`");
        }
        return Ok(Linter::combine_with_environment(
            [js, obsidian],
            environment,
        )?);
    }
    let enabled = expectation
        .rules
        .iter()
        .map(|id| glass_lint_core::RuleId::parse(id.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    let js_ids = enabled
        .iter()
        .filter(|id| id.as_str().starts_with("js:"))
        .cloned()
        .collect::<Vec<_>>();
    let obsidian_ids = enabled
        .iter()
        .filter(|id| id.as_str().starts_with("obsidian:"))
        .cloned()
        .collect::<Vec<_>>();
    let mut linters = Vec::new();
    if !js_ids.is_empty() {
        linters.push(Linter::with_rules(js.catalog().clone(), js_ids)?);
    }
    if !obsidian_ids.is_empty() {
        linters.push(Linter::with_rules(
            obsidian.catalog().clone(),
            obsidian_ids,
        )?);
    }
    if linters.is_empty() {
        bail!("project tool has no selected built-in rules");
    }
    if linters.len() == 1 {
        return Ok(linters.pop().expect("one linter"));
    }
    Ok(Linter::combine_with_environment(linters, environment)?)
}

fn run_project(project: &ProjectCase, expectation: &ToolExpectation) -> Result<AdapterRun> {
    let linter = configured_linter(expectation)?;
    let report = if project.filesystem {
        glass_lint_project::ProjectLoader::new(glass_lint_project::ProjectLoadOptions::default())?
            .load_and_lint(
            &linter,
            &glass_lint_project::ProjectSelection::directory(project.root.clone()),
        )?
    } else {
        let mut session = linter.begin_project(project.root.clone())?;
        let mut authored = Vec::new();
        for file in &project.files {
            authored.extend(
                session
                    .add_source(SourceFile::new(file.path.clone(), file.source.clone()))?
                    .into_iter()
                    .map(|request| (request, file.path.clone())),
            );
        }
        for resolution in &project.resolutions {
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
            session.record_resolution(request, result)?;
        }
        session.finish()?
    };
    project_report_to_run(report)
}

fn project_report_to_run(report: glass_lint_core::ProjectReport) -> Result<AdapterRun> {
    let diagnostics = report
        .files
        .iter()
        .flat_map(|file| file.parse_diagnostics.iter())
        .map(|diagnostic| diagnostic.message.clone())
        .chain(
            report
                .diagnostics
                .iter()
                .map(|diagnostic| format!("[{}] {}", diagnostic.code, diagnostic.message)),
        )
        .collect::<Vec<_>>();
    if !diagnostics.is_empty() {
        bail!("{}", diagnostics.join("; "));
    }
    let mut findings = Vec::new();
    let mut finding_locations = Vec::new();
    for file in report.files {
        for finding in file.findings {
            finding_locations.push(FindingLocation {
                primary: Some(finding.location.path.clone()),
                evidence: finding
                    .evidence
                    .iter()
                    .map(|evidence| {
                        evidence
                            .location
                            .as_ref()
                            .map(|location| location.path.clone())
                    })
                    .collect(),
            });
            findings.push(Finding {
                rule_id: finding.rule_id,
                message_id: finding.message_id,
                message: finding.message,
                severity: finding.severity,
                range: finding.location.range,
                evidence: finding
                    .evidence
                    .into_iter()
                    .map(|evidence| glass_lint_core::Evidence {
                        message: evidence.message,
                        range: evidence.location.map(|location| location.range),
                        source: evidence.source,
                    })
                    .collect(),
            });
        }
    }
    Ok(AdapterRun {
        findings,
        finding_locations,
    })
}

pub struct ExternalAdapter {
    pub name: String,
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
        let (findings, finding_locations) = self.run_protocol(case, expectation)?;
        Ok(AdapterRun {
            findings,
            finding_locations,
        })
    }

    fn run(&self, case: &Case, expectation: &ToolExpectation) -> Result<Vec<Finding>> {
        Ok(self.run_protocol(case, expectation)?.0)
    }
}

impl ExternalAdapter {
    fn run_protocol(
        &self,
        case: &Case,
        expectation: &ToolExpectation,
    ) -> Result<(Vec<Finding>, Vec<FindingLocation>)> {
        let mut child = Command::new(&self.command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("start adapter {}", self.command.display()))?;
        let request = AdapterRequest {
            protocol_version: ADAPTER_PROTOCOL_VERSION,
            case_id: case.id.clone(),
            filename: case.filename.clone(),
            language: case.language.clone(),
            source: case.source.clone(),
            rules: expectation.rules.clone(),
            config: expectation.config.clone(),
            project: case.project.as_ref().map(adapter_project),
        };
        serde_json::to_writer(
            child.stdin.as_mut().context("adapter stdin unavailable")?,
            &request,
        )?;
        child.stdin.take().unwrap().flush()?;
        let output = child.wait_with_output()?;
        if !output.status.success() {
            bail!(
                "adapter exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let response: AdapterResponse =
            serde_json::from_slice(&output.stdout).context("invalid adapter response")?;
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
        Ok((response.findings, response.finding_locations))
    }
}

fn adapter_project(project: &ProjectCase) -> AdapterProject {
    project.into()
}
