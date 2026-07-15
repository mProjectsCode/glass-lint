//! Provenance identities emitted by syntax and scope analysis.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis) enum SymbolCallProvenance {
    Global { name: String },
    Local,
    ModuleExport { module: String, export: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis) enum SymbolMemberProvenance {
    ModuleNamespace { module: String, member: String },
}
