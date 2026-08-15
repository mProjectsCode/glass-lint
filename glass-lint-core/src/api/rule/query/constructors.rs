//! Public event-query constructors and argument adapters.

use super::{
    ArgumentIndex, ArgumentMatcher, EventQuery, EventSpec, IdentitySpec, ModuleSpecifierPattern,
    QueryBuildError, QueryDecl, ValueMatcher, checked_chain, checked_module_export,
    checked_module_name, checked_name,
};

#[allow(clippy::cast_possible_truncation)]
impl EventQuery {
    /// Global call, e.g. `fetch(...)`.
    pub fn call_global(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        let name = checked_name(name)?;
        Ok(Self::from_parts(
            EventSpec::Call,
            IdentitySpec::Global { name },
        ))
    }

    /// Heuristic spelling call.
    pub fn call_heuristic(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        let name = checked_name(name)?;
        Ok(Self::from_parts(
            EventSpec::Call,
            IdentitySpec::Heuristic { name },
        ))
    }

    /// Module-export call.
    pub fn call_module(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let (module, export) = checked_module_export(module, export)?;
        Ok(Self::from_parts(
            EventSpec::Call,
            IdentitySpec::ModuleExport { module, export },
        ))
    }

    /// Package module export call.
    pub fn call_package(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let export = checked_name(export)?;
        let module = checked_package(module)?;
        Ok(Self::from_parts(
            EventSpec::Call,
            IdentitySpec::PackageModuleExport { module, export },
        ))
    }

    /// Rooted member call, e.g. `document.createElement(...)`.
    pub fn member_call_rooted(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        let chain = checked_chain(chain)?;
        let path = chain.path().clone();
        Ok(Self::from_parts(
            EventSpec::MemberCall {
                member: path.clone(),
            },
            IdentitySpec::Rooted { path },
        ))
    }

    /// Heuristic member call.
    pub fn member_call_heuristic(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        let chain = checked_chain(chain)?;
        Ok(Self::from_parts(
            EventSpec::MemberCall {
                member: chain.path().clone(),
            },
            IdentitySpec::Heuristic {
                name: chain.as_str().into(),
            },
        ))
    }

    /// Module-namespace member call.
    pub fn member_call_module(
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let module = checked_module_name(module)?;
        let path = checked_chain(member)?.into_path();
        Ok(Self::from_parts(
            EventSpec::MemberCall { member: path },
            IdentitySpec::ModuleNamespace { module },
        ))
    }

    /// Package module namespace member call.
    pub fn member_call_package(
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let path = checked_chain(member)?.into_path();
        let module = checked_package(module)?;
        Ok(Self::from_parts(
            EventSpec::MemberCall { member: path },
            IdentitySpec::PackageModuleNamespace { module },
        ))
    }

    /// Rooted member read.
    pub fn member_read_rooted(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        let path = checked_chain(chain)?.into_path();
        Ok(Self::from_parts(
            EventSpec::MemberRead {
                member: path.clone(),
            },
            IdentitySpec::Rooted { path },
        ))
    }

    /// Rooted member-property write, for example `document.onkeydown = fn`.
    pub fn property_write_rooted(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        let path = checked_chain(chain)?.into_path();
        Ok(Self::from_parts(
            EventSpec::PropertyWrite {
                property: path.clone(),
            },
            IdentitySpec::Rooted { path },
        ))
    }

    /// Module-namespace member read.
    pub fn member_read_module(
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let module = checked_module_name(module)?;
        let path = checked_chain(member)?.into_path();
        Ok(Self::from_parts(
            EventSpec::MemberRead { member: path },
            IdentitySpec::ModuleNamespace { module },
        ))
    }

    /// Package module namespace member read.
    pub fn member_read_package(
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let path = checked_chain(member)?.into_path();
        let module = checked_package(module)?;
        Ok(Self::from_parts(
            EventSpec::MemberRead { member: path },
            IdentitySpec::PackageModuleNamespace { module },
        ))
    }

    /// Import exact module specifier.
    pub fn import_exact(module: impl Into<String>) -> Result<Self, QueryBuildError> {
        let module_str: String = module.into();
        if module_str.trim().is_empty() {
            return Err(QueryBuildError::EmptyModuleSpecifier);
        }
        Ok(Self::from_parts(
            EventSpec::Import,
            IdentitySpec::LiteralString {
                predicate: module_str,
            },
        ))
    }

    /// Import package pattern.
    pub fn import_package(module: impl Into<String>) -> Result<Self, QueryBuildError> {
        let pattern = checked_package(module)?;
        Ok(Self::from_parts(
            EventSpec::Import,
            IdentitySpec::PackageSpecifier { pattern },
        ))
    }

    /// Static string reference.
    pub fn string_contains(value: impl Into<String>) -> Result<Self, QueryBuildError> {
        let value_str: String = value.into();
        if value_str.trim().is_empty() {
            return Err(QueryBuildError::EmptyStaticValue);
        }
        Ok(Self::from_parts(
            EventSpec::StringReference,
            IdentitySpec::LiteralString {
                predicate: value_str,
            },
        ))
    }

    /// Static literal containing a complete private or special-use network
    /// address. Matching is boundary-aware and performed by core's literal
    /// index rather than by substring markers.
    pub fn string_private_network_address() -> Result<Self, QueryBuildError> {
        Ok(Self::from_parts(
            EventSpec::StringReference,
            IdentitySpec::PrivateNetworkAddress,
        ))
    }

    /// Heuristic class reference.
    pub fn class_heuristic(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        let name = checked_name(name)?;
        Ok(Self::from_parts(
            EventSpec::ClassReference,
            IdentitySpec::Heuristic { name },
        ))
    }

    /// Module-export class reference.
    pub fn class_module(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let (module, export) = checked_module_export(module, export)?;
        Ok(Self::from_parts(
            EventSpec::ClassReference,
            IdentitySpec::ModuleExport { module, export },
        ))
    }

    /// Global constructor, e.g. `new URL(...)`.
    pub fn constructor_global(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        let name = checked_name(name)?;
        Ok(Self::from_parts(
            EventSpec::Construct,
            IdentitySpec::Global { name },
        ))
    }

    /// Rooted constructor, e.g. `new WebAssembly.Module(...)`.
    pub fn constructor_rooted(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        let path = checked_chain(chain)?.into_path();
        Ok(Self::from_parts(
            EventSpec::Construct,
            IdentitySpec::Rooted { path },
        ))
    }

    /// Heuristic constructor.
    pub fn constructor_heuristic(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        let name = checked_name(name)?;
        Ok(Self::from_parts(
            EventSpec::Construct,
            IdentitySpec::Heuristic { name },
        ))
    }

    /// Module-export constructor.
    pub fn constructor_module(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let (module, export) = checked_module_export(module, export)?;
        Ok(Self::from_parts(
            EventSpec::Construct,
            IdentitySpec::ModuleExport { module, export },
        ))
    }

    /// Add an argument predicate.
    pub fn with_arg(
        self,
        index: usize,
        matcher: impl Into<ArgumentMatcher>,
    ) -> Result<Self, QueryBuildError> {
        let arg_idx = ArgumentIndex::try_from_usize(index)?;
        self.with_arg_index(arg_idx, matcher)
    }

    fn with_arg_index(
        mut self,
        arg_idx: ArgumentIndex,
        matcher: impl Into<ArgumentMatcher>,
    ) -> Result<Self, QueryBuildError> {
        if !self.event.supports_arguments() {
            return Err(QueryBuildError::ArgumentsRequireCallEvent);
        }
        super::value::push_argument_constraint(
            &mut self.constraints,
            &mut self.constraint_counts,
            arg_idx,
            matcher,
        )?;
        Ok(self)
    }

    /// Add a static-string argument constraint.
    pub fn with_arg_static_string(self, index: usize) -> Result<Self, QueryBuildError> {
        let arg_idx = ArgumentIndex::try_from_usize(index)?;
        self.with_arg_index(arg_idx, ValueMatcher::static_string())
    }

    /// Add a static-string constraint with allowed values.
    pub fn with_arg_static_strings<I, S>(
        self,
        index: usize,
        values: I,
    ) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let arg_idx = ArgumentIndex::try_from_usize(index)?;
        self.with_arg_index(arg_idx, ValueMatcher::static_string().equals_any(values)?)
    }

    /// Add a static-string contains constraint.
    pub fn with_arg_static_string_contains<I, S>(
        self,
        index: usize,
        values: I,
    ) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let arg_idx = ArgumentIndex::try_from_usize(index)?;
        self.with_arg_index(arg_idx, ValueMatcher::static_string().contains_any(values)?)
    }

    /// Add an object property value constraint.
    pub fn with_arg_object_property_value(
        self,
        index: usize,
        property: impl Into<String>,
        value: ValueMatcher,
    ) -> Result<Self, QueryBuildError> {
        let arg_idx = ArgumentIndex::try_from_usize(index)?;
        self.with_arg_index(
            arg_idx,
            ArgumentMatcher::object_property_value(property, value)?,
        )
    }

    /// Add an object keys constraint.
    pub fn with_arg_object_keys<I, S>(self, index: usize, keys: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let arg_idx = ArgumentIndex::try_from_usize(index)?;
        self.with_arg_index(arg_idx, ArgumentMatcher::object_keys(keys)?)
    }

    /// Convert this event query into a [`QueryDecl`] with inferred evidence
    /// kind and symbol derived from the event and identity.
    pub fn into_query(self) -> QueryDecl {
        self.into_selection_assembly().into_event_decl()
    }
}

fn checked_package(module: impl Into<String>) -> Result<ModuleSpecifierPattern, QueryBuildError> {
    ModuleSpecifierPattern::package(module)
        .map_err(|error| QueryBuildError::InvalidScopePackage(error.to_string()))
}
