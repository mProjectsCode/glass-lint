use glass_lint_datastructures::{NamePath, NameTable, SymbolPath};
use smol_str::SmolStr;

mod private_network;
pub(in crate::analysis::matching) use private_network::private_network_match;

use crate::{
    Environment,
    analysis::matching::{
        LinkedOccurrenceView, ModuleOverlayKind,
        occurrence::{
            ModuleExportKey, ModuleOccurrences, NameOccurrences, OccurrenceIndex,
            OccurrenceSelection, Occurrences, PackageKeyPredicate, PackageMatchKind,
        },
    },
    api::{compiler::rule::IdentityConstraint, rule::ModuleSpecifierPattern},
};

/// One supported event view, holding only the occurrence buckets that are
/// meaningful for that event instead of an unrestricted set of options.
pub(super) enum EventIndexView<'a> {
    Call {
        names: &'a NameOccurrences,
        module: &'a ModuleOccurrences,
        global: &'a Occurrences,
    },
    MemberCall {
        member: &'a SymbolPath,
        paths: &'a OccurrenceIndex<NamePath>,
        module: &'a ModuleOccurrences,
        rooted: &'a OccurrenceIndex<NamePath>,
        environment: &'a Environment,
    },
    MemberRead {
        member: &'a SymbolPath,
        paths: &'a OccurrenceIndex<NamePath>,
        module: &'a ModuleOccurrences,
        rooted: &'a OccurrenceIndex<NamePath>,
        environment: &'a Environment,
    },
    PropertyWrite {
        property: &'a SymbolPath,
        writes: &'a OccurrenceIndex<NamePath>,
        environment: &'a Environment,
    },
    ClassReference {
        strings: &'a Occurrences,
        module: &'a ModuleOccurrences,
    },
    Construct {
        names: &'a NameOccurrences,
        module: &'a ModuleOccurrences,
        global: &'a Occurrences,
        rooted: &'a OccurrenceIndex<NamePath>,
        environment: &'a Environment,
    },
    Import {
        literals: &'a Occurrences,
    },
    StringReference {
        literals: &'a Occurrences,
    },
}

struct ModuleIndex<'a> {
    kind: ModuleOverlayKind,
    occurrences: &'a ModuleOccurrences,
}

struct RootedIndex<'a> {
    occurrences: &'a OccurrenceIndex<NamePath>,
    environment: &'a Environment,
}

enum AnyIndex<'a> {
    Names(&'a NameOccurrences),
    Members {
        occurrences: &'a OccurrenceIndex<NamePath>,
    },
    Strings(&'a Occurrences),
    Constructors(&'a NameOccurrences),
    Unsupported,
}

enum LiteralIndex<'a> {
    Import(&'a Occurrences),
    StringReference(&'a Occurrences),
    Unsupported,
}

struct EventIndexCapabilities<'a> {
    any: AnyIndex<'a>,
    global: Option<&'a Occurrences>,
    member: Option<&'a SymbolPath>,
    module: Option<ModuleIndex<'a>>,
    rooted: Option<RootedIndex<'a>>,
    literals: LiteralIndex<'a>,
}

impl<'a> EventIndexView<'a> {
    pub(super) fn resolve(
        &self,
        identity: &'a IdentityConstraint,
        names: &NameTable,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<OccurrenceSelection<'a>> {
        let capabilities = self.capabilities();
        match identity {
            IdentityConstraint::Any { name } => capabilities.resolve_any(name, names),
            IdentityConstraint::Global { name } => capabilities.resolve_global(name, overlay),
            IdentityConstraint::ModuleExport { module, export } => {
                capabilities.resolve_module_export(module, export, overlay)
            }
            IdentityConstraint::PackageModuleExport { module, export } => {
                capabilities.resolve_package_export(module, export, overlay)
            }
            IdentityConstraint::ModuleNamespace { module } => {
                capabilities.resolve_module_namespace(module, overlay)
            }
            IdentityConstraint::PackageModuleNamespace { module } => {
                capabilities.resolve_package_namespace(module, overlay)
            }
            IdentityConstraint::Rooted { path } => capabilities.resolve_rooted(path, names),
            IdentityConstraint::LiteralString { predicate } => {
                capabilities.resolve_literal(predicate)
            }
            IdentityConstraint::PackageSpecifier { pattern } => {
                capabilities.resolve_package_specifier(pattern)
            }
        }
    }

    fn capabilities(&self) -> EventIndexCapabilities<'a> {
        match self {
            EventIndexView::Call {
                names,
                module,
                global,
            } => EventIndexCapabilities::indexed(
                AnyIndex::Names(names),
                None,
                Some(global),
                Some((ModuleOverlayKind::Call, module)),
                None,
            ),
            EventIndexView::MemberCall {
                member,
                paths,
                module,
                rooted,
                environment,
            } => EventIndexCapabilities::indexed(
                AnyIndex::Members { occurrences: paths },
                Some(member),
                None,
                Some((ModuleOverlayKind::MemberCall, module)),
                Some((rooted, environment)),
            ),
            EventIndexView::MemberRead {
                member,
                paths,
                module,
                rooted,
                environment,
            } => EventIndexCapabilities::indexed(
                AnyIndex::Members { occurrences: paths },
                Some(member),
                None,
                Some((ModuleOverlayKind::MemberRead, module)),
                Some((rooted, environment)),
            ),
            EventIndexView::PropertyWrite {
                property,
                writes,
                environment,
            } => EventIndexCapabilities::indexed(
                AnyIndex::Members {
                    occurrences: writes,
                },
                Some(property),
                None,
                None,
                Some((writes, environment)),
            ),
            EventIndexView::ClassReference { strings, module } => EventIndexCapabilities::indexed(
                AnyIndex::Strings(strings),
                None,
                None,
                Some((ModuleOverlayKind::Class, module)),
                None,
            ),
            EventIndexView::Construct {
                names,
                module,
                global,
                rooted,
                environment,
            } => EventIndexCapabilities::indexed(
                AnyIndex::Constructors(names),
                None,
                Some(global),
                Some((ModuleOverlayKind::Constructor, module)),
                Some((rooted, environment)),
            ),
            EventIndexView::Import { literals } => {
                EventIndexCapabilities::literal(LiteralIndex::Import(literals))
            }
            EventIndexView::StringReference { literals } => {
                EventIndexCapabilities::literal(LiteralIndex::StringReference(literals))
            }
        }
    }
}

impl<'a> EventIndexCapabilities<'a> {
    fn indexed(
        any: AnyIndex<'a>,
        member: Option<&'a SymbolPath>,
        global: Option<&'a Occurrences>,
        module: Option<(ModuleOverlayKind, &'a ModuleOccurrences)>,
        rooted: Option<(&'a OccurrenceIndex<NamePath>, &'a Environment)>,
    ) -> Self {
        Self {
            any,
            global,
            member,
            module: module.map(|(kind, occurrences)| ModuleIndex { kind, occurrences }),
            rooted: rooted.map(|(occurrences, environment)| RootedIndex {
                occurrences,
                environment,
            }),
            literals: LiteralIndex::Unsupported,
        }
    }

    fn literal(literals: LiteralIndex<'a>) -> Self {
        Self {
            any: AnyIndex::Unsupported,
            global: None,
            member: None,
            module: None,
            rooted: None,
            literals,
        }
    }

    fn resolve_any(&self, name: &SmolStr, names: &NameTable) -> Option<OccurrenceSelection<'a>> {
        match &self.any {
            AnyIndex::Names(calls) => names
                .lookup(name)
                .and_then(|id| calls.get(&id))
                .map(OccurrenceSelection::indexed),
            AnyIndex::Members { occurrences } => names
                .lookup_path(self.member?)
                .and_then(|path| occurrences.get(&path))
                .map(OccurrenceSelection::indexed),
            AnyIndex::Strings(strings) => {
                strings.get(name.as_str()).map(OccurrenceSelection::indexed)
            }
            AnyIndex::Constructors(constructors) => names
                .lookup(name)
                .and_then(|id| constructors.get(&id))
                .map(OccurrenceSelection::indexed)
                .or_else(|| {
                    self.global
                        .and_then(|global| global.get(name.as_str()))
                        .map(OccurrenceSelection::indexed)
                }),
            AnyIndex::Unsupported => None,
        }
    }

    fn resolve_global(
        &self,
        name: &SmolStr,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<OccurrenceSelection<'a>> {
        let index = self.global?;
        overlay.map_or_else(
            || index.get(name).map(OccurrenceSelection::indexed),
            |overlay| overlay.resolve_global(index, name),
        )
    }

    fn resolve_module_export(
        &self,
        module: &SmolStr,
        export: &SmolStr,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<OccurrenceSelection<'a>> {
        let key = ModuleExportKey::new(module.clone(), export.clone());
        self.resolve_module_key(&key, overlay)
    }

    fn resolve_package_export(
        &self,
        module: &'a ModuleSpecifierPattern,
        export: &'a SmolStr,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<OccurrenceSelection<'a>> {
        let predicate = PackageKeyPredicate::new(module, PackageMatchKind::Export(export));
        self.resolve_package(predicate, overlay)
    }

    fn resolve_module_namespace(
        &self,
        module: &SmolStr,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<OccurrenceSelection<'a>> {
        let member = self.member?.to_string();
        let key = ModuleExportKey::new(module.clone(), member);
        self.resolve_module_key(&key, overlay)
    }

    fn resolve_package_namespace(
        &self,
        module: &'a ModuleSpecifierPattern,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<OccurrenceSelection<'a>> {
        let member = self.member?;
        let predicate = PackageKeyPredicate::new(module, PackageMatchKind::Namespace(member));
        self.resolve_package(predicate, overlay)
    }

    fn resolve_rooted(
        &self,
        path: &'a SymbolPath,
        names: &NameTable,
    ) -> Option<OccurrenceSelection<'a>> {
        let expected = names.lookup_path(path)?;
        let rooted = self.rooted.as_ref()?;
        rooted.occurrences.matching(|key| {
            rooted
                .environment
                .global_object_name_paths_match(key, &expected, names)
        })
    }

    fn resolve_literal(&self, predicate: &str) -> Option<OccurrenceSelection<'a>> {
        match &self.literals {
            LiteralIndex::Import(literals) => literals
                .get(&SmolStr::new(predicate))
                .map(OccurrenceSelection::indexed),
            LiteralIndex::StringReference(literals) => {
                if predicate == crate::api::rule::query::PRIVATE_NETWORK_LITERAL {
                    literals.matching(|literal| private_network_match(literal).is_some())
                } else {
                    literals.matching(|literal| literal.contains(predicate))
                }
            }
            LiteralIndex::Unsupported => None,
        }
    }

    fn resolve_package_specifier(
        &self,
        pattern: &ModuleSpecifierPattern,
    ) -> Option<OccurrenceSelection<'a>> {
        match &self.literals {
            LiteralIndex::Import(literals) | LiteralIndex::StringReference(literals) => {
                literals.matching(|specifier| pattern.matches(specifier))
            }
            LiteralIndex::Unsupported => None,
        }
    }

    fn resolve_module_key(
        &self,
        key: &ModuleExportKey,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<OccurrenceSelection<'a>> {
        let module = self.module.as_ref()?;
        overlay.map_or_else(
            || {
                module
                    .occurrences
                    .get(key)
                    .map(OccurrenceSelection::indexed)
            },
            |overlay| overlay.resolve_module(module.kind, module.occurrences, key),
        )
    }

    fn resolve_package(
        &self,
        predicate: PackageKeyPredicate<'a>,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<OccurrenceSelection<'a>> {
        let module = self.module.as_ref()?;
        Some(match overlay {
            Some(overlay) => overlay.resolve_package(module.kind, module.occurrences, predicate),
            None => OccurrenceSelection::BorrowedPackage(
                module.occurrences.package_candidates(predicate),
            ),
        })
    }
}

#[cfg(test)]
mod tests;
