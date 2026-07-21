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
use crate::api::rule::{MatcherBuildError, ModuleSpecifierPattern, validation};

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

/// Exhaustive view of matcher families. Keeping this dispatch in the owning
/// type makes adding a family a compile-time edit at one canonical list.
macro_rules! matcher_families {
    ($(($variant:ident, $field:ident, $ty:ty)),* $(,)?) => {
        const MATCHER_FAMILY_COUNT: usize = [$(stringify!($variant)),*].len();

        pub(crate) enum MatcherFamily<'a> {
            $($variant(&'a [$ty]),)*
        }

        pub(crate) enum MatcherFamilyMut<'a> {
            $($variant(&'a mut Vec<$ty>),)*
        }

        impl MatcherSet {
            pub(crate) fn families(&self) -> [MatcherFamily<'_>; MATCHER_FAMILY_COUNT] {
                [
                    $(MatcherFamily::$variant(&self.$field[..]),)*
                ]
            }

            pub(crate) fn families_mut(&mut self) -> [MatcherFamilyMut<'_>; MATCHER_FAMILY_COUNT] {
                [
                    $(MatcherFamilyMut::$variant(&mut self.$field),)*
                ]
            }
        }
    };
}

matcher_families! {
    (Calls, calls, CallMatcher),
    (MemberCalls, member_calls, MemberCallMatcher),
    (MemberReads, member_reads, MemberReadMatcher),
    (Imports, imports, String),
    (PackageImports, package_imports, ModuleSpecifierPattern),
    (StringContains, string_contains, String),
    (Classes, classes, ClassMatcher),
    (Constructors, constructors, ConstructorMatcher),
    (Flows, flows, ObjectFlowMatcher),
    (ReturnedMemberCalls, returned_member_calls, ReturnedMemberCallMatcher),
    (ReturnedMemberReads, returned_member_reads, ReturnedMemberReadMatcher),
    (InstanceMemberCalls, instance_member_calls, InstanceMemberCallMatcher),
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
    ) -> Result<Self, MatcherBuildError> {
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
    ) -> Result<Self, MatcherBuildError> {
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
    ) -> Result<Self, MatcherBuildError> {
        MemberReadMatcher::package_member(module, member).map(Into::into)
    }

    pub fn import(module: impl Into<String>) -> Self {
        Self::Import(module.into())
    }

    pub fn package_import(module: impl Into<String>) -> Result<Self, MatcherBuildError> {
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
    ) -> Result<Self, MatcherBuildError> {
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
    ) -> Result<Self, MatcherBuildError> {
        ConstructorMatcher::package_export(module, export).map(Into::into)
    }

    pub fn returned_member_call(source: impl Into<String>, member: impl Into<String>) -> Self {
        Self::ReturnedMemberCall(ReturnedMemberCallMatcher::new(source, member))
    }

    pub fn returned_member_read(source: impl Into<String>, member: impl Into<String>) -> Self {
        Self::ReturnedMemberRead(ReturnedMemberReadMatcher::new(source, member))
    }

    pub fn instance_member_call(
        module: impl Into<String>,
        export: impl Into<String>,
        member: impl Into<String>,
    ) -> Self {
        Self::InstanceMemberCall(InstanceMemberCallMatcher::new(module, export, member))
    }

    pub fn package_instance_member_call(
        module: impl Into<String>,
        export: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, MatcherBuildError> {
        let module = module.into();
        let pattern = ModuleSpecifierPattern::package(module.clone())?;
        Ok(Self::InstanceMemberCall(InstanceMemberCallMatcher::with_package(
            module,
            pattern,
            export,
            member,
        )))
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
    pub fn validate(&self) -> Result<(), MatcherBuildError> {
        validation::validate(self)
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
        flow.normalize();
    }
    values.sort_by(|left, right| left.symbol().cmp(right.symbol()));
    values.dedup();
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
