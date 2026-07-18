//! Declarative matcher vocabulary for calls, members, values, and object flow.
//!
//! Matchers distinguish heuristic spelling from rooted/global/module
//! provenance. Builder APIs are ergonomic, while [`MatcherSet::validate`] and
//! normalization enforce the precision and boundedness contract.

mod call;
mod derived;
pub mod flow;
mod member;
pub use call::*;
pub use derived::*;
pub use flow::*;
pub use member::*;

use super::ModuleSpecifierPattern;

#[derive(Debug, Clone, Default)]
/// Collection of matcher families before validation and normalization.
pub struct MatcherSet {
    /// Direct callable matchers.
    pub calls: Vec<CallMatcher>,
    /// Member-call matchers.
    pub member_calls: Vec<MemberCallMatcher>,
    /// Member-read matchers.
    pub member_reads: Vec<MemberReadMatcher>,
    /// Imported module specifier matchers.
    pub imports: Vec<String>,
    /// Boundary-aware package-root module patterns.
    pub package_imports: Vec<ModuleSpecifierPattern>,
    /// Static literal matchers.
    pub string_contains: Vec<String>,
    /// Class matchers.
    pub classes: Vec<ClassMatcher>,
    /// Constructor matchers.
    pub constructors: Vec<ConstructorMatcher>,
    /// Object lifecycle flow matchers.
    pub flows: Vec<ObjectFlowMatcher>,
    /// Returned-object member-call matchers.
    pub returned_member_calls: Vec<ReturnedMemberCallMatcher>,
    /// Returned-object member-read matchers.
    pub returned_member_reads: Vec<ReturnedMemberReadMatcher>,
    /// Module-export instance member-call matchers.
    pub instance_member_calls: Vec<InstanceMemberCallMatcher>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// One typed matcher declaration in a rule.
pub enum Matcher {
    /// Direct callable matcher.
    Call(CallMatcher),
    /// Member-call matcher.
    MemberCall(MemberCallMatcher),
    /// Member-read matcher.
    MemberRead(MemberReadMatcher),
    /// Module import matcher.
    Import(String),
    /// Package-root module matcher.
    PackageImport(ModuleSpecifierPattern),
    /// Static string matcher.
    StringContains(String),
    /// Class matcher.
    Class(ClassMatcher),
    /// Constructor matcher.
    Constructor(ConstructorMatcher),
    /// Object-flow matcher.
    ObjectFlow(ObjectFlowMatcher),
    /// Returned-object member-call matcher.
    ReturnedMemberCall(ReturnedMemberCallMatcher),
    /// Returned-object member-read matcher.
    ReturnedMemberRead(ReturnedMemberReadMatcher),
    /// Exported-instance member-call matcher.
    InstanceMemberCall(InstanceMemberCallMatcher),
}

impl Matcher {
    pub fn global_call(name: impl Into<String>) -> Self {
        CallMatcher::global(name).into()
    }

    pub fn heuristic_call(name: impl Into<String>) -> Self {
        CallMatcher::heuristic(name).into()
    }

    pub fn module_call(module: impl Into<String>, export: impl Into<String>) -> Self {
        CallMatcher::module_export(module, export).into()
    }

    pub fn package_call(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, String> {
        CallMatcher::package_export(module, export).map(Into::into)
    }

    pub fn heuristic_member_call(chain: impl Into<String>) -> Self {
        MemberCallMatcher::heuristic(chain).into()
    }

    pub fn rooted_member_call(chain: impl Into<String>) -> Self {
        MemberCallMatcher::rooted(chain).into()
    }

    pub fn module_member_call(module: impl Into<String>, member: impl Into<String>) -> Self {
        MemberCallMatcher::module_member(module, member).into()
    }

    pub fn package_member_call(
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, String> {
        MemberCallMatcher::package_member(module, member).map(Into::into)
    }

    pub fn heuristic_member_read(chain: impl Into<String>) -> Self {
        MemberReadMatcher::heuristic(chain).into()
    }

    pub fn rooted_member_read(chain: impl Into<String>) -> Self {
        MemberReadMatcher::rooted(chain).into()
    }

    pub fn module_member_read(module: impl Into<String>, member: impl Into<String>) -> Self {
        MemberReadMatcher::module_member(module, member).into()
    }

    pub fn package_member_read(
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, String> {
        MemberReadMatcher::package_member(module, member).map(Into::into)
    }

    pub fn import(module: impl Into<String>) -> Self {
        Self::Import(module.into())
    }

    pub fn package_import(module: impl Into<String>) -> Result<Self, String> {
        ModuleSpecifierPattern::package(module).map(Self::PackageImport)
    }

    pub fn string_contains(value: impl Into<String>) -> Self {
        Self::StringContains(value.into())
    }

    pub fn heuristic_class(name: impl Into<String>) -> Self {
        ClassMatcher::heuristic(name).into()
    }

    pub fn module_class(module: impl Into<String>, export: impl Into<String>) -> Self {
        ClassMatcher::module_export(module, export).into()
    }

    pub fn package_class(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, String> {
        ClassMatcher::package_export(module, export).map(Into::into)
    }

    pub fn heuristic_constructor(name: impl Into<String>) -> Self {
        ConstructorMatcher::heuristic(name).into()
    }

    pub fn global_constructor(name: impl Into<String>) -> Self {
        ConstructorMatcher::global(name).into()
    }

    pub fn module_constructor(module: impl Into<String>, export: impl Into<String>) -> Self {
        ConstructorMatcher::module_export(module, export).into()
    }

    pub fn package_constructor(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, String> {
        ConstructorMatcher::package_export(module, export).map(Into::into)
    }

    pub fn returned_member_call(source: impl Into<String>, member: impl Into<String>) -> Self {
        Self::ReturnedMemberCall(ReturnedMemberCallMatcher {
            source: source.into(),
            member: member.into(),
        })
    }

    pub fn returned_member_read(source: impl Into<String>, member: impl Into<String>) -> Self {
        Self::ReturnedMemberRead(ReturnedMemberReadMatcher {
            source: source.into(),
            member: member.into(),
        })
    }

    pub fn instance_member_call(
        module: impl Into<String>,
        export: impl Into<String>,
        member: impl Into<String>,
    ) -> Self {
        Self::InstanceMemberCall(InstanceMemberCallMatcher {
            module: module.into(),
            module_pattern: None,
            export: export.into(),
            member: member.into(),
        })
    }

    pub fn package_instance_member_call(
        module: impl Into<String>,
        export: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, String> {
        let module = module.into();
        let pattern = ModuleSpecifierPattern::package(module.clone())?;
        Ok(Self::InstanceMemberCall(InstanceMemberCallMatcher {
            module,
            module_pattern: Some(pattern),
            export: export.into(),
            member: member.into(),
        }))
    }
}

impl From<CallMatcher> for Matcher {
    fn from(value: CallMatcher) -> Self {
        Self::Call(value)
    }
}
impl From<MemberCallMatcher> for Matcher {
    fn from(value: MemberCallMatcher) -> Self {
        Self::MemberCall(value)
    }
}
impl From<MemberReadMatcher> for Matcher {
    fn from(value: MemberReadMatcher) -> Self {
        Self::MemberRead(value)
    }
}
impl From<ClassMatcher> for Matcher {
    fn from(value: ClassMatcher) -> Self {
        Self::Class(value)
    }
}
impl From<ConstructorMatcher> for Matcher {
    fn from(value: ConstructorMatcher) -> Self {
        Self::Constructor(value)
    }
}
impl From<ObjectFlowMatcher> for Matcher {
    fn from(value: ObjectFlowMatcher) -> Self {
        Self::ObjectFlow(value)
    }
}
impl From<ReturnedMemberCallMatcher> for Matcher {
    fn from(value: ReturnedMemberCallMatcher) -> Self {
        Self::ReturnedMemberCall(value)
    }
}
impl From<ReturnedMemberReadMatcher> for Matcher {
    fn from(value: ReturnedMemberReadMatcher) -> Self {
        Self::ReturnedMemberRead(value)
    }
}
impl From<InstanceMemberCallMatcher> for Matcher {
    fn from(value: InstanceMemberCallMatcher) -> Self {
        Self::InstanceMemberCall(value)
    }
}

impl MatcherSet {
    /// Assemble a matcher collection from typed declarations.
    pub fn from_matchers(matchers: Vec<Matcher>) -> Self {
        let mut api_matcher = Self::default();
        for matcher in matchers {
            api_matcher.push(matcher);
        }
        api_matcher
    }

    /// Flatten matcher families in their canonical family order.
    pub fn into_matchers(self) -> Vec<Matcher> {
        self.calls
            .into_iter()
            .map(Matcher::Call)
            .chain(self.member_calls.into_iter().map(Matcher::MemberCall))
            .chain(self.member_reads.into_iter().map(Matcher::MemberRead))
            .chain(self.imports.into_iter().map(Matcher::Import))
            .chain(self.package_imports.into_iter().map(Matcher::PackageImport))
            .chain(
                self.string_contains
                    .into_iter()
                    .map(Matcher::StringContains),
            )
            .chain(self.classes.into_iter().map(Matcher::Class))
            .chain(self.constructors.into_iter().map(Matcher::Constructor))
            .chain(self.flows.into_iter().map(Matcher::ObjectFlow))
            .chain(
                self.returned_member_calls
                    .into_iter()
                    .map(Matcher::ReturnedMemberCall),
            )
            .chain(
                self.returned_member_reads
                    .into_iter()
                    .map(Matcher::ReturnedMemberRead),
            )
            .chain(
                self.instance_member_calls
                    .into_iter()
                    .map(Matcher::InstanceMemberCall),
            )
            .collect()
    }

    /// Append one typed matcher to its corresponding family.
    pub fn push(&mut self, matcher: Matcher) {
        match matcher {
            Matcher::Call(value) => self.calls.push(value),
            Matcher::MemberCall(value) => self.member_calls.push(value),
            Matcher::MemberRead(value) => self.member_reads.push(value),
            Matcher::Import(value) => self.imports.push(value),
            Matcher::PackageImport(value) => self.package_imports.push(value),
            Matcher::StringContains(value) => self.string_contains.push(value),
            Matcher::Class(value) => self.classes.push(value),
            Matcher::Constructor(value) => self.constructors.push(value),
            Matcher::ObjectFlow(value) => self.flows.push(value),
            Matcher::ReturnedMemberCall(value) => self.returned_member_calls.push(value),
            Matcher::ReturnedMemberRead(value) => self.returned_member_reads.push(value),
            Matcher::InstanceMemberCall(value) => self.instance_member_calls.push(value),
        }
    }

    /// Validate all declarations without normalizing or mutating them.
    pub fn validate(&self) -> Result<(), String> {
        super::validation::validate(self)
    }

    /// Whether no matcher family contains a declaration.
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
            && self.member_calls.is_empty()
            && self.member_reads.is_empty()
            && self.imports.is_empty()
            && self.package_imports.is_empty()
            && self.string_contains.is_empty()
            && self.classes.is_empty()
            && self.constructors.is_empty()
            && self.flows.is_empty()
            && self.returned_member_calls.is_empty()
            && self.returned_member_reads.is_empty()
            && self.instance_member_calls.is_empty()
    }

    #[must_use]
    pub fn normalized(self) -> Self {
        self.normalize()
    }
}

pub fn normalize_flows(values: &mut Vec<ObjectFlowMatcher>) {
    for flow in values.iter_mut() {
        flow.symbol = flow.symbol.trim().to_string();
        for source in &mut flow.sources {
            source.call.normalize();
        }
        if let Some(condition) = &mut flow.condition {
            condition.normalize();
        }
        if let Some(completion) = &mut flow.completion {
            completion.normalize();
        }
    }
    values.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    values.dedup();
}

impl FlowCondition {
    fn normalize(&mut self) {
        let events = match self {
            Self::AnyOf(events) | Self::AllOf(events) => events,
        };
        for event in events {
            event.normalize();
        }
    }
}

impl ObjectEventMatcher {
    fn normalize(&mut self) {
        match self {
            Self::PropertyWrite { property, value } => {
                *property = property.trim().to_string();
                value.normalize();
            }
            Self::MemberCall { member, arguments } => {
                *member = member.trim().to_string();
                ArgumentConstraint::normalize_all(arguments);
            }
        }
    }
}

impl FlowCompletion {
    fn normalize(&mut self) {
        if let Self::AnySink(sinks) = self {
            for sink in sinks {
                match sink {
                    FlowSinkMatcher::ArgumentOf { call, .. }
                    | FlowSinkMatcher::AnyArgumentOf { call } => call.normalize(),
                }
            }
        }
    }
}

pub fn normalize_strings(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    for value in values.iter_mut() {
        *value = value.trim().to_string();
    }
    values.sort();
    values.dedup();
}

pub fn normalize_member_chain(value: &str) -> String {
    crate::analysis::canonical_symbol_path(value)
}

pub fn canonical_rooted_chain(value: &str) -> &str {
    value.strip_prefix("this.").unwrap_or(value)
}
