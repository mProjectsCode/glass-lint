use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

use glass_lint_datastructures::{NamePath, NameTable, SymbolPath};
use smol_str::SmolStr;

use crate::{
    Environment,
    analysis::{
        matching::{
            LinkedOccurrenceView, ModuleOverlayKind,
            occurrence::{
                CandidateOccurrences, ModuleExportKey, ModuleOccurrences, NameOccurrences,
                OccurrenceIndex, Occurrences, PackageKeyPredicate, PackageMatchKind,
            },
        },
        value::matches_global_object_alias_with,
    },
    api::{
        compiler::rule::{EventPredicate, IdentityConstraint},
        rule::ModuleSpecifierPattern,
    },
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
        paths: &'a OccurrenceIndex<NamePath>,
        module: &'a ModuleOccurrences,
        rooted: &'a OccurrenceIndex<NamePath>,
        environment: &'a Environment,
    },
    MemberRead {
        paths: &'a OccurrenceIndex<NamePath>,
        module: &'a ModuleOccurrences,
        rooted: &'a OccurrenceIndex<NamePath>,
        environment: &'a Environment,
    },
    PropertyWrite {
        paths: &'a OccurrenceIndex<NamePath>,
        rooted: &'a OccurrenceIndex<NamePath>,
        environment: &'a Environment,
    },
    ClassReference {
        strings: &'a Occurrences,
        module: &'a ModuleOccurrences,
    },
    Construct {
        names: &'a NameOccurrences,
        strings: &'a Occurrences,
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
        event: &'a EventPredicate,
        names: &NameTable,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<CandidateOccurrences<'a>> {
        match identity {
            IdentityConstraint::Any { name, .. } => self.resolve_any(name, event, names),
            IdentityConstraint::Global { name, .. } => self.resolve_global(name, overlay),
            IdentityConstraint::ModuleExport { module, export } => {
                self.resolve_module_export(module, export, overlay)
            }
            IdentityConstraint::PackageModuleExport { module, export } => {
                self.resolve_package_export(module, export, overlay)
            }
            IdentityConstraint::ModuleNamespace { module } => {
                self.resolve_module_namespace(module, event, overlay)
            }
            IdentityConstraint::PackageModuleNamespace { module } => {
                self.resolve_package_namespace(module, event, overlay)
            }
            IdentityConstraint::Rooted { path } => self.resolve_rooted(path, event, names),
            IdentityConstraint::LiteralString { predicate } => {
                self.resolve_literal(predicate, event)
            }
            IdentityConstraint::PackageSpecifier { pattern } => {
                self.resolve_package_specifier(pattern)
            }
        }
    }

    fn resolve_any(
        &self,
        name: &SmolStr,
        event: &'a EventPredicate,
        names: &NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
        match self {
            EventIndexView::Call { names: calls, .. } => names
                .lookup(name)
                .and_then(|id| calls.get(&id))
                .map(CandidateOccurrences::Indexed),
            EventIndexView::MemberCall { paths, .. }
            | EventIndexView::MemberRead { paths, .. }
            | EventIndexView::PropertyWrite { paths, .. } => {
                let member = member_path(event)?;
                names
                    .lookup_path(member)
                    .and_then(|path| paths.get(&path))
                    .map(CandidateOccurrences::Indexed)
            }
            EventIndexView::ClassReference { strings, .. } => strings
                .get(name.as_str())
                .map(CandidateOccurrences::Indexed),
            EventIndexView::Construct {
                names: constructors,
                strings,
                ..
            } => names
                .lookup(name)
                .and_then(|id| constructors.get(&id))
                .map(CandidateOccurrences::Indexed)
                .or_else(|| {
                    strings
                        .get(name.as_str())
                        .map(CandidateOccurrences::Indexed)
                }),
            EventIndexView::Import { .. } | EventIndexView::StringReference { .. } => None,
        }
    }

    fn resolve_global(
        &self,
        name: &SmolStr,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<CandidateOccurrences<'a>> {
        let index = self.global_index()?;
        overlay.map_or_else(
            || index.get(name).map(CandidateOccurrences::Indexed),
            |overlay| overlay.resolve_global(index, name),
        )
    }

    fn resolve_module_export(
        &self,
        module: &SmolStr,
        export: &SmolStr,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<CandidateOccurrences<'a>> {
        let key = ModuleExportKey::new(module.clone(), export.clone());
        self.resolve_module_key(&key, overlay)
    }

    fn resolve_package_export(
        &self,
        module: &'a ModuleSpecifierPattern,
        export: &'a SmolStr,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<CandidateOccurrences<'a>> {
        let predicate = PackageKeyPredicate::new(module, PackageMatchKind::Export(export));
        self.resolve_package(predicate, overlay)
    }

    fn resolve_module_namespace(
        &self,
        module: &SmolStr,
        event: &'a EventPredicate,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<CandidateOccurrences<'a>> {
        let member = member_path(event)?.to_string();
        let key = ModuleExportKey::new(module.clone(), member);
        self.resolve_module_key(&key, overlay)
    }

    fn resolve_package_namespace(
        &self,
        module: &'a ModuleSpecifierPattern,
        event: &'a EventPredicate,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<CandidateOccurrences<'a>> {
        let (EventPredicate::MemberCall { member } | EventPredicate::MemberRead { member }) = event
        else {
            return None;
        };
        let predicate = PackageKeyPredicate::new(module, PackageMatchKind::Namespace(member));
        self.resolve_package(predicate, overlay)
    }

    fn resolve_rooted(
        &self,
        path: &'a SymbolPath,
        event: &'a EventPredicate,
        names: &NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
        let expected = names.lookup_path(path)?;
        let (rooted, environment) = match self {
            EventIndexView::Construct {
                rooted,
                environment,
                ..
            } if matches!(event, EventPredicate::Construct) => (rooted, environment),
            EventIndexView::MemberCall {
                rooted,
                environment,
                ..
            } if matches!(event, EventPredicate::MemberCall { .. }) => (rooted, environment),
            EventIndexView::MemberRead {
                rooted,
                environment,
                ..
            } if matches!(event, EventPredicate::MemberRead { .. }) => (rooted, environment),
            EventIndexView::PropertyWrite {
                rooted,
                environment,
                ..
            } if matches!(event, EventPredicate::PropertyWrite { .. }) => (rooted, environment),
            _ => return None,
        };
        rooted.matching(|key| matches_global_object_alias_with(key, &expected, names, environment))
    }

    fn resolve_literal(
        &self,
        predicate: &str,
        event: &EventPredicate,
    ) -> Option<CandidateOccurrences<'a>> {
        match (self, event) {
            (EventIndexView::Import { literals }, EventPredicate::Import) => literals
                .get(&SmolStr::new(predicate))
                .map(CandidateOccurrences::Indexed),
            (EventIndexView::StringReference { literals }, EventPredicate::StringReference) => {
                if predicate == crate::api::rule::query::PRIVATE_NETWORK_LITERAL {
                    literals.matching(|literal| private_network_match(literal).is_some())
                } else {
                    literals.matching(|literal| literal.contains(predicate))
                }
            }
            _ => None,
        }
    }

    fn resolve_package_specifier(
        &self,
        pattern: &ModuleSpecifierPattern,
    ) -> Option<CandidateOccurrences<'a>> {
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
    ) -> Option<CandidateOccurrences<'a>> {
        let (kind, index) = self.module_view()?;
        overlay.map_or_else(
            || index.get(key).map(CandidateOccurrences::Indexed),
            |overlay| overlay.resolve_module(kind, index, key),
        )
    }

    fn resolve_package(
        &self,
        predicate: PackageKeyPredicate<'a>,
        overlay: Option<&'a LinkedOccurrenceView<'a>>,
    ) -> Option<CandidateOccurrences<'a>> {
        let (kind, index) = self.module_view()?;
        Some(match overlay {
            Some(overlay) => overlay.resolve_package(kind, index, predicate),
            None => CandidateOccurrences::BorrowedPackage(
                index.package_candidates(predicate, None, None),
            ),
        })
    }

    fn module_view(&self) -> Option<(ModuleOverlayKind, &'a ModuleOccurrences)> {
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
            _ => None,
        }
    }

    fn global_index(&self) -> Option<&'a Occurrences> {
        match self {
            EventIndexView::Call { global, .. } | EventIndexView::Construct { global, .. } => {
                Some(global)
            }
            _ => None,
        }
    }
}

fn member_path(event: &EventPredicate) -> Option<&SymbolPath> {
    match event {
        EventPredicate::MemberCall { member }
        | EventPredicate::MemberRead { member }
        | EventPredicate::PropertyWrite { property: member } => Some(member),
        _ => None,
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
