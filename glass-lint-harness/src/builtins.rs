//! Provider/profile selection shared by adapters and profiling.

use anyhow::{Result, bail};
use glass_lint_core::{Linter, RuleBaseline, RuleId, RuleOverride, RuleSelection, RuleState};

#[derive(Clone, Copy)]
/// Built-in rule provider available to the harness.
pub enum BuiltinProvider {
    Js,
    Node,
    Electron,
    Obsidian,
}

#[derive(Clone, Copy)]
/// Precision profile used to construct a provider linter.
pub enum BuiltinProfile {
    Recommended,
    Heuristic,
}

/// Construct one built-in provider linter with the caller's host environment.
/// All harness entry points use this boundary so profile and adapter behavior
/// cannot drift when provider defaults change.
pub fn linter(provider: BuiltinProvider, profile: BuiltinProfile) -> Linter {
    let baseline = profile_baseline(profile);
    Linter::new(config(provider).with_rules(RuleSelection::new(baseline)))
        .expect("built-in catalogs are valid")
}

/// Construct a built-in provider linter with exactly the requested rules.
///
/// Explicit selection starts from [`RuleBaseline::None`], as required by
/// harness fixtures and profiles. Every rule must use the namespace owned by
/// `provider`; this keeps provider selection and catalog composition together
/// at the harness boundary.
pub fn linter_for_rules(
    provider: BuiltinProvider,
    rules: impl IntoIterator<Item = RuleId>,
) -> Result<Linter> {
    let selection =
        rules
            .into_iter()
            .try_fold(RuleSelection::new(RuleBaseline::None), |selection, id| {
                if !provider.accepts_rule(&id) {
                    bail!(
                        "rule `{id}` is not part of the `{}` built-in catalog",
                        provider.name()
                    );
                }
                Ok::<_, anyhow::Error>(
                    selection.with_override(RuleOverride::new(id.to_string(), RuleState::Enabled)?),
                )
            })?;
    Linter::new(config(provider).with_rules(selection)).map_err(Into::into)
}

fn profile_baseline(profile: BuiltinProfile) -> RuleBaseline {
    match profile {
        BuiltinProfile::Recommended => RuleBaseline::recommended(),
        BuiltinProfile::Heuristic => RuleBaseline::All,
    }
}

fn config(provider: BuiltinProvider) -> glass_lint_core::LinterConfig {
    match provider {
        BuiltinProvider::Js => glass_lint_js::JavaScriptTarget::Js.config(),
        BuiltinProvider::Node => glass_lint_js::JavaScriptTarget::Node.config(),
        BuiltinProvider::Electron => glass_lint_js::JavaScriptTarget::Electron.config(),
        BuiltinProvider::Obsidian => glass_lint_obsidian::obsidian_config(),
    }
}

impl BuiltinProvider {
    fn accepts_rule(self, id: &RuleId) -> bool {
        match self {
            Self::Js => glass_lint_js::JavaScriptTarget::Js.accepts_rule(id),
            Self::Node => glass_lint_js::JavaScriptTarget::Node.accepts_rule(id),
            Self::Electron => glass_lint_js::JavaScriptTarget::Electron.accepts_rule(id),
            Self::Obsidian => glass_lint_obsidian::accepts_rule(id),
        }
    }

    pub(crate) fn accepts_profile_rule(self, id: &RuleId) -> bool {
        match self {
            Self::Js => glass_lint_js::JavaScriptTarget::Js.accepts_rule(id),
            Self::Obsidian => glass_lint_obsidian::accepts_isolated_rule(id),
            Self::Node | Self::Electron => false,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Js => "js",
            Self::Node => "node",
            Self::Electron => "electron",
            Self::Obsidian => "obsidian",
        }
    }
}

#[allow(dead_code)]
pub fn provider(name: &str) -> Result<BuiltinProvider> {
    match name {
        "js" => Ok(BuiltinProvider::Js),
        "node" => Ok(BuiltinProvider::Node),
        "electron" => Ok(BuiltinProvider::Electron),
        "obsidian" => Ok(BuiltinProvider::Obsidian),
        _ => bail!("unsupported built-in provider {name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_rules_follow_target_catalog_composition() {
        let browser_rule = RuleId::parse("browser:browser.file-dialog").unwrap();
        assert!(linter_for_rules(BuiltinProvider::Obsidian, [browser_rule.clone()]).is_ok());
        assert!(linter_for_rules(BuiltinProvider::Js, [browser_rule]).is_err());
    }
}
