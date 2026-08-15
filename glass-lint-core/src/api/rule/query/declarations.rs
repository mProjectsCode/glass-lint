use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use super::QueryBuildError;

fn is_chain_malformed(chain: &str) -> bool {
    chain.trim().is_empty()
        || chain.contains("..")
        || chain.starts_with('.')
        || chain.ends_with('.')
}

/// A member chain validated once at the query boundary, retaining both its
/// canonical display spelling and the parsed symbol path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct MemberChain {
    display: String,
    path: SymbolPath,
}

impl MemberChain {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, QueryBuildError> {
        let value = value.into();
        if is_chain_malformed(&value) {
            return Err(QueryBuildError::MalformedChain(value));
        }
        let path = SymbolPath::from_chain(&value);
        if path.is_empty() {
            return Err(QueryBuildError::MalformedChain(value));
        }
        Ok(Self {
            display: path.to_string(),
            path,
        })
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.display
    }

    pub(crate) fn path(&self) -> &SymbolPath {
        &self.path
    }

    pub(crate) fn into_path(self) -> SymbolPath {
        self.path
    }
}

pub(crate) fn checked_name(value: impl Into<String>) -> Result<SmolStr, QueryBuildError> {
    let value: SmolStr = value.into().trim().to_owned().into();
    if value.trim().is_empty() {
        return Err(QueryBuildError::EmptyIdentityName);
    }
    Ok(value)
}

pub(crate) fn checked_module_name(value: impl Into<String>) -> Result<SmolStr, QueryBuildError> {
    let value: SmolStr = value.into().trim().to_owned().into();
    if value.is_empty() {
        return Err(QueryBuildError::EmptyModuleSpecifier);
    }
    Ok(value)
}

pub(crate) fn checked_module_export(
    module: impl Into<String>,
    export: impl Into<String>,
) -> Result<(SmolStr, SmolStr), QueryBuildError> {
    let module = checked_module_name(module)?;
    let export = checked_name(export)?;
    Ok((module, export))
}

pub(crate) fn checked_chain(value: impl Into<String>) -> Result<MemberChain, QueryBuildError> {
    MemberChain::parse(value)
}

pub(crate) const PRIVATE_NETWORK_LITERAL: &str = "__glass_lint_private_network_literal__";
pub(crate) const PRIVATE_NETWORK_EVIDENCE_SYMBOL: &str = "private network address";
