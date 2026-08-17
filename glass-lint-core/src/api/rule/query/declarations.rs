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

fn checked_text(
    value: impl Into<String>,
    empty_error: QueryBuildError,
) -> Result<String, QueryBuildError> {
    let value = value.into();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(empty_error);
    }
    Ok(trimmed.to_owned())
}

pub(crate) fn checked_name(value: impl Into<String>) -> Result<SmolStr, QueryBuildError> {
    checked_text(value, QueryBuildError::EmptyIdentityName).map(Into::into)
}

pub(crate) fn checked_module_name(value: impl Into<String>) -> Result<SmolStr, QueryBuildError> {
    checked_text(value, QueryBuildError::EmptyModuleSpecifier).map(Into::into)
}

pub(crate) fn checked_module_export(
    module: impl Into<String>,
    export: impl Into<String>,
) -> Result<(SmolStr, SmolStr), QueryBuildError> {
    let module = checked_module_name(module)?;
    let export = checked_name(export)?;
    Ok((module, export))
}

pub(crate) const PRIVATE_NETWORK_EVIDENCE_SYMBOL: &str = "private network address";
