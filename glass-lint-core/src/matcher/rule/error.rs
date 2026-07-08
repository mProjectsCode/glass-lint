#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum ApiRuleBuildError {
    MissingId,
    MissingLabel,
    MissingCategory,
    MissingSeverity,
    MissingConfidence,
    MissingMatcher,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiCatalogError {
    DuplicateRule(String),
}
