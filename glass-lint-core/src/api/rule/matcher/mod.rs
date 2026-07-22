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

use super::{normalization, validation};
use crate::api::rule::{MatcherBuildError, ModuleSpecifierPattern};

#[derive(Debug, Clone, Default)]
/// Collection of matcher families before validation and normalization.
pub struct MatcherSet {
    /// Direct callable matchers.
    pub(crate) calls: Vec<CallMatcher>,
    /// Member-call matchers.
    pub(crate) member_calls: Vec<MemberCallMatcher>,
    /// Member-read matchers.
    pub(crate) member_reads: Vec<MemberReadMatcher>,
    /// Imported module specifier matchers.
    pub(crate) imports: Vec<String>,
    /// Boundary-aware package-root module patterns.
    pub(crate) package_imports: Vec<ModuleSpecifierPattern>,
    /// Static literal matchers.
    pub(crate) string_contains: Vec<String>,
    /// Class matchers.
    pub(crate) classes: Vec<ClassMatcher>,
    /// Constructor matchers.
    pub(crate) constructors: Vec<ConstructorMatcher>,
    /// Object lifecycle flow matchers.
    pub(crate) flows: Vec<ObjectFlowMatcher>,
    /// Returned-object member-call matchers.
    pub(crate) returned_member_calls: Vec<ReturnedMemberCallMatcher>,
    /// Returned-object member-read matchers.
    pub(crate) returned_member_reads: Vec<ReturnedMemberReadMatcher>,
    /// Module-export instance member-call matchers.
    pub(crate) instance_member_calls: Vec<InstanceMemberCallMatcher>,
}

macro_rules! generate_from {
    (from $ty:ty, $matcher_variant:ident) => {
        impl From<$ty> for Matcher {
            fn from(value: $ty) -> Self {
                Self::$matcher_variant(value)
            }
        }
    };
    (no_from $ty:ty, $matcher_variant:ident) => {};
}

/// Canonical declaration for every matcher family. Adding a family to this
/// list requires a normalize function, a validate function, and a lowering
/// function — the macro generates From impls (for families with unique types
/// marked `from`), push/emptiness/flatten dispatch, and the normalize/validate
/// methods that call each family's hooks.  Omission from validation,
/// normalization, or lowering is a compile-time error.
macro_rules! matcher_families {
    ($(($family_variant:ident, $matcher_variant:ident, $field:ident, $ty:ty,
        normalize($norm_fn:path),
        validate($val_fn:path),
        $from_state:ident
    )),* $(,)?) => {
        const MATCHER_FAMILY_COUNT: usize = [$(stringify!($family_variant)),*].len();

        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum Matcher {
            $(
                $matcher_variant($ty),
            )*
        }

        pub(crate) enum MatcherFamily<'a> {
            $($family_variant(&'a [$ty]),)*
        }

        pub(crate) enum MatcherFamilyMut<'a> {
            $($family_variant(&'a mut Vec<$ty>),)*
        }

        impl MatcherSet {
            pub(crate) fn families(&self) -> [MatcherFamily<'_>; MATCHER_FAMILY_COUNT] {
                [
                    $(MatcherFamily::$family_variant(&self.$field[..]),)*
                ]
            }

            pub(crate) fn families_mut(&mut self) -> [MatcherFamilyMut<'_>; MATCHER_FAMILY_COUNT] {
                [
                    $(MatcherFamilyMut::$family_variant(&mut self.$field),)*
                ]
            }

            pub fn into_matchers(self) -> Vec<Matcher> {
                let mut result = Vec::new();
                $(result.extend(self.$field.into_iter().map(Matcher::$matcher_variant));)*
                result
            }

            pub fn push(&mut self, matcher: Matcher) {
                match matcher {
                    $(Matcher::$matcher_variant(value) => self.$field.push(value),)*
                }
            }

            pub fn is_empty(&self) -> bool {
                $(self.$field.is_empty())&&*
            }

            fn normalize(mut self) -> Self {
                for family in self.families_mut() {
                    match family {
                        $(MatcherFamilyMut::$family_variant(values) => $norm_fn(values),)*
                    }
                }
                self
            }

            fn validate_inner(&self) -> Result<(), MatcherBuildError> {
                for family in self.families() {
                    match family {
                        $(MatcherFamily::$family_variant(values) => $val_fn(values)?,)*
                    }
                }
                Ok(())
            }
        }

        $(
            generate_from!($from_state $ty, $matcher_variant);
        )*
    };
}

matcher_families! {
    (Calls, Call, calls, CallMatcher,
        normalize(normalization::normalize_calls),
        validate(validation::validate_calls),
        from),
    (MemberCalls, MemberCall, member_calls, MemberCallMatcher,
        normalize(normalization::normalize_member_calls),
        validate(validation::validate_member_calls),
        from),
    (MemberReads, MemberRead, member_reads, MemberReadMatcher,
        normalize(normalization::normalize_member_reads),
        validate(validation::validate_member_reads),
        from),
    (Imports, Import, imports, String,
        normalize(normalize_strings),
        validate(validation::validate_literal_strings),
        no_from),
    (PackageImports, PackageImport, package_imports, ModuleSpecifierPattern,
        normalize(normalization::normalize_package_imports),
        validate(validation::validate_package_imports),
        no_from),
    (StringContains, StringContains, string_contains, String,
        normalize(normalize_strings),
        validate(validation::validate_literal_strings),
        no_from),
    (Classes, Class, classes, ClassMatcher,
        normalize(ClassMatcher::normalize_all),
        validate(validation::validate_classes),
        from),
    (Constructors, Constructor, constructors, ConstructorMatcher,
        normalize(ConstructorMatcher::normalize_all),
        validate(validation::validate_constructors),
        from),
    (Flows, ObjectFlow, flows, ObjectFlowMatcher,
        normalize(normalize_flows),
        validate(validation::validate_flows),
        from),
    (ReturnedMemberCalls, ReturnedMemberCall, returned_member_calls, ReturnedMemberCallMatcher,
        normalize(ReturnedMemberCallMatcher::normalize_all),
        validate(validation::validate_returned_member_calls),
        from),
    (ReturnedMemberReads, ReturnedMemberRead, returned_member_reads, ReturnedMemberReadMatcher,
        normalize(ReturnedMemberReadMatcher::normalize_all),
        validate(validation::validate_returned_member_reads),
        from),
    (InstanceMemberCalls, InstanceMemberCall, instance_member_calls, InstanceMemberCallMatcher,
        normalize(InstanceMemberCallMatcher::normalize_all),
        validate(validation::validate_instance_member_calls),
        from),
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
        Ok(Self::InstanceMemberCall(
            InstanceMemberCallMatcher::with_package(module, pattern, export, member),
        ))
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

    /// Validate all declarations without normalizing or mutating them.
    pub fn validate(&self) -> Result<(), MatcherBuildError> {
        self.validate_inner()
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
