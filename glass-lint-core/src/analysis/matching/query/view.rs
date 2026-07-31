use std::{
    collections::{BTreeMap, BTreeSet},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

use glass_lint_datastructures::{NamePath, NameTable, SymbolPath};
use smol_str::SmolStr;

use crate::{
    Environment,
    analysis::{
        matching::occurrence::{
            BorrowedOccurrenceIter, BorrowedPackageOccurrenceIter, CandidateOccurrences,
            ModuleExportKey, ModuleOccurrences, NameOccurrences, Occurrence, OccurrenceIndex,
            Occurrences, PackageKeyPredicate, PackageMatchKind,
        },
        value::matches_global_object_alias_with,
    },
    api::{
        compiler::rule::{EventPredicate, IdentityConstraint},
        rule::ModuleSpecifierPattern,
    },
};

fn module_occurrences<'a, K: Ord>(
    base: &'a OccurrenceIndex<K>,
    overlay: Option<&'a BTreeMap<K, Vec<&'a [Occurrence]>>>,
    masked: bool,
    key: &K,
) -> Option<CandidateOccurrences<'a>> {
    if let Some(overlay_slices) = overlay.and_then(|overlay| overlay.get(key)) {
        return Some(CandidateOccurrences::Borrowed(BorrowedOccurrenceIter::new(
            None,
            overlay_slices.as_slice(),
        )));
    }
    if !masked && let Some(base_slice) = base.get(key) {
        return Some(CandidateOccurrences::Indexed(base_slice));
    }
    None
}

fn package_occurrences<'a>(
    base: &'a ModuleOccurrences,
    overlay: Option<&'a BTreeMap<ModuleExportKey, Vec<&'a [Occurrence]>>>,
    masked: Option<&'a BTreeSet<ModuleExportKey>>,
    predicate: PackageKeyPredicate<'a>,
) -> CandidateOccurrences<'a> {
    let iter = BorrowedPackageOccurrenceIter::new(predicate, masked, base.as_map(), overlay);
    CandidateOccurrences::BorrowedPackage(iter)
}

fn merged_or_indexed<'a>(
    base: Option<&'a [Occurrence]>,
    overlay: Option<&'a Vec<&'a [Occurrence]>>,
) -> Option<CandidateOccurrences<'a>> {
    match (base, overlay) {
        (Some(base_slice), Some(overlay_slices)) => Some(CandidateOccurrences::Borrowed(
            BorrowedOccurrenceIter::new(Some(base_slice), overlay_slices.as_slice()),
        )),
        (Some(slice), None) => Some(CandidateOccurrences::Indexed(slice)),
        (None, Some(slices)) => Some(CandidateOccurrences::Borrowed(BorrowedOccurrenceIter::new(
            None,
            slices.as_slice(),
        ))),
        (None, None) => None,
    }
}

pub(super) struct EventIndexView<'a> {
    pub(super) name_any: Option<&'a NameOccurrences>,
    pub(super) string_any: Option<&'a Occurrences>,
    pub(super) path_any: Option<&'a OccurrenceIndex<NamePath>>,
    pub(super) module: Option<&'a ModuleOccurrences>,
    pub(super) global: Option<&'a Occurrences>,
    pub(super) rooted: Option<&'a OccurrenceIndex<NamePath>>,
    pub(super) literal: Option<&'a Occurrences>,
    pub(super) module_overlay: Option<&'a BTreeMap<ModuleExportKey, Vec<&'a [Occurrence]>>>,
    pub(super) global_overlay: Option<&'a BTreeMap<SmolStr, Vec<&'a [Occurrence]>>>,
    pub(super) masked: Option<&'a BTreeSet<ModuleExportKey>>,
    pub(super) environment: &'a Environment,
}

impl<'a> EventIndexView<'a> {
    pub(super) fn base(environment: &'a Environment) -> Self {
        Self {
            name_any: None,
            string_any: None,
            path_any: None,
            module: None,
            global: None,
            rooted: None,
            literal: None,
            module_overlay: None,
            global_overlay: None,
            masked: None,
            environment,
        }
    }

    pub(super) fn resolve(
        &self,
        identity: &'a IdentityConstraint,
        event: &'a EventPredicate,
        names: &NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
        match identity {
            IdentityConstraint::Any { name, .. } => self.resolve_any(name, event, names),
            IdentityConstraint::Global { name, .. } => self.resolve_global(name),
            IdentityConstraint::ModuleExport { module, export } => {
                self.resolve_module_export(module, export)
            }
            IdentityConstraint::PackageModuleExport { module, export } => {
                self.resolve_package_export(module, export)
            }
            IdentityConstraint::ModuleNamespace { module } => {
                self.resolve_module_namespace(module, event)
            }
            IdentityConstraint::PackageModuleNamespace { module } => {
                self.resolve_package_namespace(module, event)
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
        if let Some(name_index) = self.name_any
            && let Some(id) = names.lookup(name)
            && let Some(result) = name_index.get(&id)
        {
            return Some(CandidateOccurrences::Indexed(result));
        }
        if let (
            Some(path_index),
            EventPredicate::MemberCall { member }
            | EventPredicate::MemberRead { member }
            | EventPredicate::PropertyWrite { property: member },
        ) = (self.path_any, event)
            && let Some(path) = names.lookup_path(member)
            && let Some(result) = path_index.get(&path)
        {
            return Some(CandidateOccurrences::Indexed(result));
        }
        if let Some(string_index) = self.string_any
            && let Some(result) = string_index.get(name.as_str())
        {
            return Some(CandidateOccurrences::Indexed(result));
        }
        None
    }

    fn resolve_global(&self, name: &SmolStr) -> Option<CandidateOccurrences<'a>> {
        merged_or_indexed(
            self.global?.get(name),
            self.global_overlay.and_then(|o| o.get(name)),
        )
    }

    fn resolve_module_export(
        &self,
        module: &SmolStr,
        export: &SmolStr,
    ) -> Option<CandidateOccurrences<'a>> {
        let key = ModuleExportKey::new(module.clone(), export.clone());
        module_occurrences(
            self.module?,
            self.module_overlay,
            self.masked.is_some_and(|masked| masked.contains(&key)),
            &key,
        )
    }

    fn resolve_package_export(
        &self,
        module: &'a ModuleSpecifierPattern,
        export: &'a SmolStr,
    ) -> Option<CandidateOccurrences<'a>> {
        Some(package_occurrences(
            self.module?,
            self.module_overlay,
            self.masked,
            PackageKeyPredicate::new(module, PackageMatchKind::Export(export)),
        ))
    }

    fn resolve_module_namespace(
        &self,
        module: &SmolStr,
        event: &'a EventPredicate,
    ) -> Option<CandidateOccurrences<'a>> {
        let key = match event {
            EventPredicate::MemberCall { member }
            | EventPredicate::MemberRead { member }
            | EventPredicate::PropertyWrite { property: member } => {
                ModuleExportKey::new(module.clone(), member.to_string())
            }
            _ => return None,
        };
        module_occurrences(
            self.module?,
            self.module_overlay,
            self.masked.is_some_and(|masked| masked.contains(&key)),
            &key,
        )
    }

    fn resolve_package_namespace(
        &self,
        module: &'a ModuleSpecifierPattern,
        event: &'a EventPredicate,
    ) -> Option<CandidateOccurrences<'a>> {
        let (EventPredicate::MemberCall { member } | EventPredicate::MemberRead { member }) = event
        else {
            return None;
        };
        Some(package_occurrences(
            self.module?,
            self.module_overlay,
            self.masked,
            PackageKeyPredicate::new(module, PackageMatchKind::Namespace(member)),
        ))
    }

    fn resolve_rooted(
        &self,
        path: &'a SymbolPath,
        event: &'a EventPredicate,
        names: &NameTable,
    ) -> Option<CandidateOccurrences<'a>> {
        let (EventPredicate::MemberCall { member: _ }
        | EventPredicate::MemberRead { member: _ }
        | EventPredicate::PropertyWrite { property: _ }) = event
        else {
            return None;
        };
        let expected = names.lookup_path(path)?;
        self.rooted?.matching(|key| {
            matches_global_object_alias_with(key, &expected, names, self.environment)
        })
    }

    fn resolve_literal(
        &self,
        predicate: &str,
        event: &EventPredicate,
    ) -> Option<CandidateOccurrences<'a>> {
        match event {
            EventPredicate::Import => self
                .literal?
                .get(&SmolStr::new(predicate))
                .map(CandidateOccurrences::Indexed),
            EventPredicate::StringReference => {
                if predicate == crate::api::rule::query::PRIVATE_NETWORK_LITERAL {
                    self.literal?
                        .matching(|literal| contains_private_network_address(literal))
                } else {
                    self.literal?
                        .matching(|literal| literal.contains(predicate))
                }
            }
            _ => None,
        }
    }

    fn resolve_package_specifier(
        &self,
        pattern: &ModuleSpecifierPattern,
    ) -> Option<CandidateOccurrences<'a>> {
        self.literal?
            .matching(|specifier| pattern.matches(specifier))
    }
}

fn contains_private_network_address(value: &str) -> bool {
    contains_localhost(value) || contains_private_ipv4(value) || contains_private_ipv6(value)
}

fn contains_localhost(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    let bytes = lowered.as_bytes();
    lowered.match_indices("localhost").any(|(index, _)| {
        let before = index.checked_sub(1).and_then(|i| bytes.get(i));
        let after = bytes.get(index + "localhost".len());
        before.is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'.')
            && after.is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'.')
    })
}

fn contains_private_ipv4(value: &str) -> bool {
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
        let boundary =
            end == bytes.len() || (!bytes[end].is_ascii_alphanumeric() && bytes[end] != b'.');
        if candidate.matches('.').count() == 3
            && boundary
            && IpAddr::from_str(candidate).is_ok_and(|ip| match ip {
                IpAddr::V4(ip) => private_ipv4(ip),
                IpAddr::V6(ip) => private_ipv6(ip),
            })
        {
            return true;
        }
        start = end.max(start + 1);
    }
    false
}

fn contains_private_ipv6(value: &str) -> bool {
    value
        .split(|character: char| {
            character.is_whitespace()
                || matches!(character, '"' | '\'' | '(' | ')' | ',' | '=' | '?' | '#')
        })
        .any(|token| {
            let token = token
                .trim_start_matches("http://")
                .trim_start_matches("https://")
                .split('/')
                .next()
                .unwrap_or(token);
            let host = token.strip_prefix('[').map_or(token, |bracketed| {
                bracketed.split(']').next().unwrap_or(bracketed)
            });
            host.contains(':') && Ipv6Addr::from_str(host).is_ok_and(private_ipv6)
        })
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
