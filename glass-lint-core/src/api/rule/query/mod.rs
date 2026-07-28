//! Declaration-owned query semantics.
//!
//! Types in this module are provider-neutral and validated at construction by
//! the builder. They represent authored intent without exposing compiler IR.
//! The compiler layer lowers these into execution types.

use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::rule::ModuleSpecifierPattern;

/// Declaration-owned identity specification for an event.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum IdentitySpec {
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
}

impl IdentitySpec {
    /// Return a display-oriented string for the identity name.
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
        }
    }
}

/// Declaration-owned event kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum EventSpec {
    Call,
    Construct,
    MemberCall { member: SymbolPath },
    MemberRead { member: SymbolPath },
    ClassReference,
    Import,
    StringReference,
}

/// Declaration-owned subject relationship.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum SubjectSpec {
    /// The event is directly on the identity.
    Direct,
    /// The event is on an object returned from a producer.
    ReturnedFrom { producer: Box<IdentitySpec> },
    /// The event is on an instance created by a constructor.
    InstanceOf { constructor: Box<IdentitySpec> },
}
