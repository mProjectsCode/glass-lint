use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

use glass_lint_datastructures::{NamePath, NameTable, SymbolPath};
use smol_str::SmolStr;

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

pub(in crate::analysis::matching) fn private_network_match(value: &str) -> Option<(usize, usize)> {
    contains_localhost(value)
        .or_else(|| contains_private_ipv4(value))
        .or_else(|| contains_private_ipv6(value))
}

fn contains_localhost(value: &str) -> Option<(usize, usize)> {
    let lowered = value.to_ascii_lowercase();
    let bytes = lowered.as_bytes();
    lowered.match_indices("localhost").find_map(|(index, _)| {
        let before = index.checked_sub(1).and_then(|i| bytes.get(i));
        let after = bytes.get(index + "localhost".len());
        (before.is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'.')
            && after.is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'.'))
        .then_some((index, index + "localhost".len()))
    })
}

fn contains_private_ipv4(value: &str) -> Option<(usize, usize)> {
    let bytes = value.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        if !bytes[start].is_ascii_digit()
            || (start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'.'))
        {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
            end += 1;
        }
        let candidate = &value[start..end];
        let before_is_boundary = start == 0
            || (!bytes[start - 1].is_ascii_alphanumeric()
                && bytes[start - 1] != b'.'
                && bytes[start - 1] != b'\\');
        let boundary =
            end == bytes.len() || (!bytes[end].is_ascii_alphanumeric() && bytes[end] != b'.');
        if candidate.matches('.').count() == 3
            && before_is_boundary
            && boundary
            && IpAddr::from_str(candidate).is_ok_and(|ip| match ip {
                IpAddr::V4(ip) => private_ipv4(ip),
                IpAddr::V6(ip) => private_ipv6(ip),
            })
        {
            return Some((start, end));
        }
        start = end.max(start + 1);
    }
    None
}

fn contains_private_ipv6(value: &str) -> Option<(usize, usize)> {
    let mut token_start = 0;
    for (index, character) in value.char_indices() {
        if character.is_whitespace()
            || matches!(character, '"' | '\'' | '(' | ')' | ',' | '=' | '?' | '#')
        {
            if let Some(found) = private_ipv6_token(value, token_start, index) {
                return Some(found);
            }
            token_start = index + character.len_utf8();
        }
    }
    private_ipv6_token(value, token_start, value.len())
}

fn private_ipv6_token(value: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let token = &value[start..end];
    let (token, token_start) = token.strip_prefix("http://").map_or_else(
        || {
            token
                .strip_prefix("https://")
                .map_or((token, start), |host| (host, start + "https://".len()))
        },
        |host| (host, start + "http://".len()),
    );
    let slash = token.find('/').unwrap_or(token.len());
    let token = &token[..slash];
    let (host, host_start) = token
        .strip_prefix('[')
        .map_or((token, token_start), |host| (host, token_start + 1));
    let host_end = host.find(']').unwrap_or(host.len());
    let host = &host[..host_end];
    let before = value[..host_start].chars().next_back();
    let after = value[host_start + host.len()..].chars().next();
    if before.is_some_and(|character| matches!(character, '?' | '\\'))
        || after.is_some_and(|character| matches!(character, '?' | '\\'))
    {
        return None;
    }
    host.contains(':')
        .then(|| Ipv6Addr::from_str(host).ok())
        .flatten()
        .filter(|ip| private_ipv6(*ip))
        .map(|_| (host_start, host_start + host.len()))
}

fn private_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, d] = ip.octets();
    (a == 0 && b == 0 && c == 0 && d == 0)
        || a == 10
        || a == 127
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (18..=19).contains(&b))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
}

fn private_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || ip == Ipv6Addr::LOCALHOST
        || ip.is_unspecified()
        || ip.to_ipv4().is_some_and(private_ipv4)
}

#[cfg(test)]
mod tests {
    use super::private_network_match;

    #[test]
    fn regex_ipv6_syntax_is_not_an_address() {
        assert_eq!(
            private_network_match(r"^\s*(?:\(?(?:GMT|UTC)\s?)?([+-])(\d{1,2})(?::?(\d{2}))?\)?"),
            None
        );
        assert_eq!(
            private_network_match(
                r"([0-9]{4})\-([0-9]{1,2})\-([0-9]{1,2})(?:T([0-9]{1,2}):([0-9]{1,2})(?::([0-9]{1,2}))?)?"
            ),
            None
        );
    }

    #[test]
    fn returns_the_address_span() {
        assert_eq!(
            private_network_match("https://192.168.1.2:8080"),
            Some((8, 19))
        );
        assert_eq!(private_network_match("http://[::1]/"), Some((8, 11)));
    }
}
