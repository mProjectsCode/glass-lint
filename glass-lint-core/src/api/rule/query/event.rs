use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::rule::ModuleSpecifierPattern;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub(crate) enum IdentitySpec {
    Global {
        name: SmolStr,
    },
    Heuristic {
        name: SmolStr,
    },
    ModuleExport {
        module: SmolStr,
        export: SmolStr,
    },
    PackageModuleExport {
        module: ModuleSpecifierPattern,
        export: SmolStr,
    },
    ModuleNamespace {
        module: SmolStr,
    },
    PackageModuleNamespace {
        module: ModuleSpecifierPattern,
    },
    Rooted {
        path: SymbolPath,
    },
    LiteralString {
        predicate: String,
    },
    PackageSpecifier {
        pattern: ModuleSpecifierPattern,
    },
    PrivateNetworkAddress,
}

impl IdentitySpec {
    pub fn display_name(&self) -> String {
        match self {
            Self::Global { name } | Self::Heuristic { name } => name.to_string(),
            Self::ModuleExport { module, export } => format!("{module}.{export}"),
            Self::PackageModuleExport { module, export } => format!("{module}.{export}"),
            Self::ModuleNamespace { module } => module.to_string(),
            Self::PackageModuleNamespace { module } => module.to_string(),
            Self::Rooted { path } => path.to_string(),
            Self::LiteralString { predicate } => predicate.clone(),
            Self::PackageSpecifier { pattern } => pattern.to_string(),
            Self::PrivateNetworkAddress => super::PRIVATE_NETWORK_EVIDENCE_SYMBOL.to_owned(),
        }
    }

    pub fn diagnostic_name(&self) -> &'static str {
        match self {
            Self::Global { .. } => "global",
            Self::Heuristic { .. } => "heuristic",
            Self::ModuleExport { .. } => "module_export",
            Self::PackageModuleExport { .. } => "package_module_export",
            Self::ModuleNamespace { .. } => "module_namespace",
            Self::PackageModuleNamespace { .. } => "package_module_namespace",
            Self::Rooted { .. } => "rooted",
            Self::LiteralString { .. } => "literal",
            Self::PackageSpecifier { .. } => "package_specifier",
            Self::PrivateNetworkAddress => "private_network_address",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub(crate) enum EventSpec {
    Call,
    Construct,
    MemberCall { member: SymbolPath },
    MemberRead { member: SymbolPath },
    PropertyWrite { property: SymbolPath },
    ClassReference,
    Import,
    StringReference,
}

impl EventSpec {
    /// Return whether this event can carry argument constraints.
    pub(crate) const fn supports_arguments(&self) -> bool {
        matches!(self, Self::Call | Self::MemberCall { .. })
    }

    /// Return the compiler variable type represented by this event kind.
    pub(crate) fn variable_type(&self) -> super::VarType {
        match self {
            Self::Call | Self::Construct => super::VarType::CallEvent,
            Self::MemberCall { .. } | Self::MemberRead { .. } | Self::PropertyWrite { .. } => {
                super::VarType::MemberEvent
            }
            Self::ClassReference | Self::Import | Self::StringReference => super::VarType::Event,
        }
    }

    pub fn diagnostic_name(&self) -> &'static str {
        match self {
            Self::Call => "call",
            Self::Construct => "construct",
            Self::MemberCall { .. } => "member_call",
            Self::MemberRead { .. } => "member_read",
            Self::PropertyWrite { .. } => "property_write",
            Self::ClassReference => "class",
            Self::Import => "import",
            Self::StringReference => "string",
        }
    }
}
