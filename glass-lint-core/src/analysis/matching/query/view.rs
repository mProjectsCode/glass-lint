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

impl<'a> EventIndexView<'a> {
    pub(super) fn resolve(
        &self,
        identity: &'a IdentityConstraint,
        names: &NameTable,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<OccurrenceSelection<'a>> {
        match identity {
            IdentityConstraint::Any { name } => self.resolve_any(name, names),
            IdentityConstraint::Global { name } => self.resolve_global(name, overlay),
            IdentityConstraint::ModuleExport { module, export } => {
                self.resolve_module_export(module, export, overlay)
            }
            IdentityConstraint::PackageModuleExport { module, export } => {
                self.resolve_package_export(module, export, overlay)
            }
            IdentityConstraint::ModuleNamespace { module } => {
                self.resolve_module_namespace(module, overlay)
            }
            IdentityConstraint::PackageModuleNamespace { module } => {
                self.resolve_package_namespace(module, overlay)
            }
            IdentityConstraint::Rooted { path } => self.resolve_rooted(path, names),
            IdentityConstraint::LiteralString { predicate } => self.resolve_literal(predicate),
            IdentityConstraint::PackageSpecifier { pattern } => {
                self.resolve_package_specifier(pattern)
            }
            IdentityConstraint::PrivateNetworkAddress => self.resolve_private_network(),
        }
    }

    fn members(&self) -> Option<(&'a SymbolPath, &'a OccurrenceIndex<NamePath>)> {
        match self {
            EventIndexView::MemberCall { member, paths, .. }
            | EventIndexView::MemberRead { member, paths, .. } => Some((member, paths)),
            EventIndexView::PropertyWrite {
                property, writes, ..
            } => Some((property, writes)),
            _ => None,
        }
    }

    fn member(&self) -> Option<&'a SymbolPath> {
        match self {
            EventIndexView::MemberCall { member, .. }
            | EventIndexView::MemberRead { member, .. } => Some(member),
            EventIndexView::PropertyWrite { property, .. } => Some(property),
            _ => None,
        }
    }

    fn global(&self) -> Option<&'a Occurrences> {
        match self {
            EventIndexView::Call { global, .. } | EventIndexView::Construct { global, .. } => {
                Some(global)
            }
            _ => None,
        }
    }

    fn module(&self) -> Option<(ModuleOverlayKind, &'a ModuleOccurrences)> {
        match self {
            EventIndexView::Call { module, .. } => Some((ModuleOverlayKind::Call, module)),
            EventIndexView::MemberCall { module, .. } => {
                Some((ModuleOverlayKind::MemberCall, module))
            }
            EventIndexView::MemberRead { module, .. } => {
                Some((ModuleOverlayKind::MemberRead, module))
            }
            EventIndexView::ClassReference { module, .. } => {
                Some((ModuleOverlayKind::Class, module))
            }
            EventIndexView::Construct { module, .. } => {
                Some((ModuleOverlayKind::Constructor, module))
            }
            EventIndexView::PropertyWrite { .. }
            | EventIndexView::Import { .. }
            | EventIndexView::StringReference { .. } => None,
        }
    }

    fn rooted(&self) -> Option<(&'a OccurrenceIndex<NamePath>, &'a Environment)> {
        match self {
            EventIndexView::MemberCall {
                rooted,
                environment,
                ..
            }
            | EventIndexView::MemberRead {
                rooted,
                environment,
                ..
            }
            | EventIndexView::Construct {
                rooted,
                environment,
                ..
            } => Some((rooted, environment)),
            EventIndexView::PropertyWrite {
                writes,
                environment,
                ..
            } => Some((writes, environment)),
            EventIndexView::Call { .. }
            | EventIndexView::ClassReference { .. }
            | EventIndexView::Import { .. }
            | EventIndexView::StringReference { .. } => None,
        }
    }
}

impl<'a> EventIndexView<'a> {
    fn resolve_any(&self, name: &SmolStr, names: &NameTable) -> Option<OccurrenceSelection<'a>> {
        match self {
            EventIndexView::Call { names: calls, .. } => names
                .lookup(name)
                .and_then(|id| calls.get(&id))
                .map(OccurrenceSelection::indexed),
            EventIndexView::Construct {
                names: constructors,
                global,
                ..
            } => names
                .lookup(name)
                .and_then(|id| constructors.get(&id))
                .map(OccurrenceSelection::indexed)
                .or_else(|| global.get(name.as_str()).map(OccurrenceSelection::indexed)),
            EventIndexView::ClassReference { strings, .. } => {
                strings.get(name.as_str()).map(OccurrenceSelection::indexed)
            }
            _ => self.members().and_then(|(member, occurrences)| {
                names
                    .lookup_path(member)
                    .and_then(|path| occurrences.get(&path))
                    .map(OccurrenceSelection::indexed)
            }),
        }
    }

    fn resolve_global(
        &self,
        name: &SmolStr,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<OccurrenceSelection<'a>> {
        let index = self.global()?;
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
        let member = self.member()?.to_string();
        let key = ModuleExportKey::new(module.clone(), member);
        self.resolve_module_key(&key, overlay)
    }

    fn resolve_package_namespace(
        &self,
        module: &'a ModuleSpecifierPattern,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<OccurrenceSelection<'a>> {
        let member = self.member()?;
        let predicate = PackageKeyPredicate::new(module, PackageMatchKind::Namespace(member));
        self.resolve_package(predicate, overlay)
    }

    fn resolve_rooted(
        &self,
        path: &'a SymbolPath,
        names: &NameTable,
    ) -> Option<OccurrenceSelection<'a>> {
        let expected = names.lookup_path(path)?;
        let (rooted, environment) = self.rooted()?;
        rooted.matching(|key| environment.global_object_name_paths_match(key, &expected, names))
    }

    fn resolve_literal(&self, predicate: &str) -> Option<OccurrenceSelection<'a>> {
        match self {
            EventIndexView::Import { literals } => literals
                .get(&SmolStr::new(predicate))
                .map(OccurrenceSelection::indexed),
            EventIndexView::StringReference { literals } => {
                literals.matching(|literal| literal.contains(predicate))
            }
            _ => None,
        }
    }

    fn resolve_private_network(&self) -> Option<OccurrenceSelection<'a>> {
        match self {
            EventIndexView::StringReference { literals } => {
                literals.matching(|literal| private_network_match(literal).is_some())
            }
            _ => None,
        }
    }

    fn resolve_package_specifier(
        &self,
        pattern: &ModuleSpecifierPattern,
    ) -> Option<OccurrenceSelection<'a>> {
        match self {
            EventIndexView::Import { literals } | EventIndexView::StringReference { literals } => {
                literals.matching(|specifier| pattern.matches(specifier))
            }
            _ => None,
        }
    }

    fn resolve_module_key(
        &self,
        key: &ModuleExportKey,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<OccurrenceSelection<'a>> {
        let (kind, occurrences) = self.module()?;
        overlay.map_or_else(
            || occurrences.get(key).map(OccurrenceSelection::indexed),
            |overlay| overlay.resolve_module(kind, occurrences, key),
        )
    }

    fn resolve_package(
        &self,
        predicate: PackageKeyPredicate<'a>,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<OccurrenceSelection<'a>> {
        let (kind, occurrences) = self.module()?;
        Some(match overlay {
            Some(overlay) => overlay.resolve_package(kind, occurrences, predicate),
            None => OccurrenceSelection::BorrowedPackage(occurrences.package_candidates(predicate)),
        })
    }
}

#[cfg(test)]
mod tests;
