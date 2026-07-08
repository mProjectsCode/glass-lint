use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use glass_lint_core::Finding;

use crate::types::{
    ADAPTER_PROTOCOL_VERSION, AdapterRequest, AdapterResponse, Case, ToolExpectation,
};

pub trait Adapter {
    fn name(&self) -> &str;
    fn version(&self) -> Result<String>;
    fn run(&self, case: &Case, expectation: &ToolExpectation) -> Result<Vec<Finding>>;
}

pub struct GlassLintAdapter;

impl Adapter for GlassLintAdapter {
    fn name(&self) -> &str {
        "glass-lint"
    }

    fn version(&self) -> Result<String> {
        Ok(env!("CARGO_PKG_VERSION").into())
    }

    fn run(&self, case: &Case, expectation: &ToolExpectation) -> Result<Vec<Finding>> {
        let configured = glass_lint_obsidian::heuristic_linter();
        let enabled = expectation
            .rules
            .iter()
            .map(|id| glass_lint_core::RuleId::parse(id.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        let linter = glass_lint_core::Linter::with_rules(configured.catalog().clone(), enabled)?;
        let report = linter.lint(&case.source, &case.filename);
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

    fn run(&self, case: &Case, expectation: &ToolExpectation) -> Result<Vec<Finding>> {
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
            source: case.source.clone(),
            rules: expectation.rules.clone(),
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
        Ok(response.findings)
    }
}
