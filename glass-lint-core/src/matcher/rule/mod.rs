mod error;
mod matcher;
mod taxonomy;

pub use error::{ApiCatalogError, ApiRuleBuildError};
pub use matcher::{
    ApiMatcher, ArgStringMatcher, CallMatcher, CallProvenance, ClassMatcher, ConstructorMatcher,
    FlowSinkArgs, FlowValueMatcher, Matcher, MemberCallMatcher, MemberCallProvenance,
    MemberReadMatcher, MemberReadProvenance, ValueFlowConfiguration, ValueFlowMatcher,
    canonical_rooted_chain,
};
pub use taxonomy::{ApiCategory, ApiSeverity, Confidence};

#[derive(Debug, Clone)]
pub struct ApiRule {
    pub id: String,
    pub label: String,
    pub category: ApiCategory,
    pub severity: ApiSeverity,
    pub confidence: Confidence,
    pub matcher: ApiMatcher,
    pub implies: Vec<String>,
}

impl ApiRule {
    pub const EVIDENCE_LIMIT: usize = 5;

    pub fn builder(id: impl Into<String>) -> ApiRuleBuilder {
        ApiRuleBuilder {
            id: id.into(),
            label: None,
            category: None,
            severity: None,
            confidence: None,
            matcher: ApiMatcher::default(),
            implies: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApiRuleBuilder {
    id: String,
    label: Option<String>,
    category: Option<ApiCategory>,
    severity: Option<ApiSeverity>,
    confidence: Option<Confidence>,
    matcher: ApiMatcher,
    implies: Vec<String>,
}

impl ApiRuleBuilder {
    pub fn matcher(mut self, matcher: impl Into<Matcher>) -> Self {
        self.matcher.push(matcher.into());
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn category(mut self, category: impl Into<ApiCategory>) -> Self {
        self.category = Some(category.into());
        self
    }

    pub fn severity(mut self, severity: ApiSeverity) -> Self {
        self.severity = Some(severity);
        self
    }

    pub fn confidence(mut self, confidence: Confidence) -> Self {
        self.confidence = Some(confidence);
        self
    }

    pub fn calls<I, S>(mut self, calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.matcher.calls.extend(
            calls
                .into_iter()
                .map(Into::into)
                .map(CallMatcher::unqualified),
        );
        self
    }

    pub fn global_calls<I, S>(mut self, calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.matcher
            .calls
            .extend(calls.into_iter().map(Into::into).map(CallMatcher::global));
        self
    }

    pub fn module_calls<I, S>(mut self, module: impl Into<String>, exports: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let module = module.into();
        self.matcher.calls.extend(
            exports
                .into_iter()
                .map(Into::into)
                .map(|name| CallMatcher::module_export(module.clone(), name)),
        );
        self
    }

    pub fn member_calls<I, S>(mut self, member_calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.matcher.member_calls.extend(
            member_calls
                .into_iter()
                .map(Into::into)
                .map(MemberCallMatcher::chain),
        );
        self
    }

    pub fn rooted_member_calls<I, S>(mut self, member_calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.matcher.member_calls.extend(
            member_calls
                .into_iter()
                .map(Into::into)
                .map(MemberCallMatcher::rooted_chain),
        );
        self
    }

    pub fn module_member_calls<I, S>(mut self, module: impl Into<String>, members: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let module = module.into();
        self.matcher.member_calls.extend(
            members
                .into_iter()
                .map(Into::into)
                .map(|member| MemberCallMatcher::module_member(module.clone(), member)),
        );
        self
    }

    pub fn member_reads<I, S>(mut self, member_reads: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.matcher.member_reads.extend(
            member_reads
                .into_iter()
                .map(Into::into)
                .map(MemberReadMatcher::chain),
        );
        self
    }

    pub fn rooted_member_reads<I, S>(mut self, member_reads: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.matcher.member_reads.extend(
            member_reads
                .into_iter()
                .map(Into::into)
                .map(MemberReadMatcher::rooted_chain),
        );
        self
    }

    pub fn module_member_reads<I, S>(mut self, module: impl Into<String>, members: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let module = module.into();
        self.matcher.member_reads.extend(
            members
                .into_iter()
                .map(Into::into)
                .map(|member| MemberReadMatcher::module_member(module.clone(), member)),
        );
        self
    }

    pub fn imports<I, S>(mut self, imports: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.matcher
            .imports
            .extend(imports.into_iter().map(Into::into));
        self
    }

    pub fn string_literals<I, S>(mut self, string_literals: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.matcher
            .string_literals
            .extend(string_literals.into_iter().map(Into::into));
        self
    }

    pub fn classes<I, S>(mut self, classes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.matcher
            .classes
            .extend(classes.into_iter().map(Into::into).map(class_matcher));
        self
    }

    pub fn constructors<I, S>(mut self, constructors: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.matcher.constructors.extend(
            constructors
                .into_iter()
                .map(Into::into)
                .map(constructor_matcher),
        );
        self
    }

    pub fn implies<I, S>(mut self, implies: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.implies.extend(implies.into_iter().map(Into::into));
        self
    }

    pub fn build(self) -> Result<ApiRule, ApiRuleBuildError> {
        let label = required_string(self.label, ApiRuleBuildError::MissingLabel)?;
        let category = self.category.ok_or(ApiRuleBuildError::MissingCategory)?;
        let severity = self.severity.ok_or(ApiRuleBuildError::MissingSeverity)?;
        let confidence = self
            .confidence
            .ok_or(ApiRuleBuildError::MissingConfidence)?;

        let id = self.id.trim().to_string();
        if id.is_empty() {
            return Err(ApiRuleBuildError::MissingId);
        }

        let matcher = self.matcher.normalized();
        let implies = normalized_strings(self.implies);
        if matcher.is_empty() {
            return Err(ApiRuleBuildError::MissingMatcher);
        }
        Ok(ApiRule {
            id,
            label,
            category,
            severity,
            confidence,
            matcher,
            implies,
        })
    }
}

fn class_matcher(value: String) -> ClassMatcher {
    if let Some((module, export)) = value.split_once('.') {
        ClassMatcher::module_export(module.to_string(), export.to_string())
    } else {
        ClassMatcher::unqualified(value)
    }
}

fn constructor_matcher(value: String) -> ConstructorMatcher {
    if let Some((module, export)) = value.split_once('.') {
        ConstructorMatcher::module_export(module.to_string(), export.to_string())
    } else {
        ConstructorMatcher::unqualified(value)
    }
}

fn required_string(
    value: Option<String>,
    missing_error: ApiRuleBuildError,
) -> Result<String, ApiRuleBuildError> {
    let value = value.ok_or(missing_error)?;
    if value.trim().is_empty() {
        return Err(missing_error);
    }

    Ok(value.trim().to_string())
}

fn normalized_strings(values: Vec<String>) -> Vec<String> {
    let mut values = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}
