use super::super::rule::{ApiMatcher, ApiRule};

#[derive(Debug, Clone)]
pub(crate) struct CompiledRule {
    #[allow(dead_code)]
    pub(crate) catalog_index: usize,
    pub(crate) matcher: ApiMatcher,
}

impl CompiledRule {
    pub(crate) fn new(catalog_index: usize, rule: &ApiRule) -> Self {
        Self {
            catalog_index,
            matcher: rule.matcher_for_compilation(),
        }
    }
}
