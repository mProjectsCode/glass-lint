//! Explicit host-environment semantics used by provenance analysis.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use glass_lint_datastructures::{Fingerprint, NameId, NamePath, NameTable, PathView};
use smol_str::SmolStr;
use swc_ecma_ast::EsReserved;

/// The globals and current- or foreign-realm global objects available to
/// analyzed code.
///
/// The default contains only stable ECMAScript globals. Browser, Node.js,
/// Electron, and provider-injected names belong in provider configurations.
///
/// Cloning is cheap: only the shared `Arc` handle is copied. Equality compares
/// the inner value, so cache-key semantics are preserved.
#[derive(Debug)]
pub struct Environment {
    inner: Arc<EnvironmentInner>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EnvironmentInner {
    global_bindings: BTreeSet<SmolStr>,
    global_objects: BTreeMap<SmolStr, GlobalObjectMembers>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Membership policy for a global object's promoted identities.
enum GlobalObjectMembers {
    /// This object promotes all currently configured globals as callable
    /// identities. Used for the current-realm global object and fully trusted
    /// aliases.
    ConfiguredGlobals,
    /// Only the listed names are promoted from this foreign-realm object.
    /// Used for window-like objects from another security context.
    Restricted(BTreeSet<SmolStr>),
}

impl GlobalObjectMembers {
    fn write_fingerprint_bytes(&self, fp: &mut Fingerprint) {
        match self {
            Self::ConfiguredGlobals => fp.write(&[0u8]),
            Self::Restricted(member_set) => {
                fp.write(&[1u8]);
                fp.write(&(member_set.len() as u64).to_le_bytes());
                for member in member_set {
                    fp.write(member.as_bytes());
                    fp.write(&[0u8]);
                }
            }
        }
    }
}

impl EnvironmentInner {
    /// Write the canonical identity used by equality and artifact caching.
    /// BTree iteration keeps the representation deterministic.
    fn write_fingerprint_bytes(&self, fp: &mut Fingerprint) {
        fp.write(&(self.global_bindings.len() as u64).to_le_bytes());
        for name in &self.global_bindings {
            fp.write(name.as_bytes());
            fp.write(&[0u8]);
        }
        fp.write(&(self.global_objects.len() as u64).to_le_bytes());
        for (name, members) in &self.global_objects {
            fp.write(name.as_bytes());
            fp.write(&[0u8]);
            members.write_fingerprint_bytes(fp);
        }
    }
}

impl Clone for Environment {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl PartialEq for Environment {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner) || *self.inner == *other.inner
    }
}

impl Eq for Environment {}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Error returned for a malformed host binding identifier.
pub struct EnvironmentError {
    name: String,
}

impl std::fmt::Display for EnvironmentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid JavaScript global identifier `{}`",
            self.name
        )
    }
}

impl std::error::Error for EnvironmentError {}

impl Default for Environment {
    fn default() -> Self {
        Self::ecmascript()
    }
}

fn is_js_identifier_start(c: char) -> bool {
    if c == '$' || c == '_' {
        return true;
    }
    swc_ecma_ast::Ident::is_valid_start(c)
}

fn is_js_identifier_continue(c: char) -> bool {
    if c == '$' || c == '_' {
        return true;
    }
    swc_ecma_ast::Ident::is_valid_continue(c)
}

impl Environment {
    fn inner(&self) -> &EnvironmentInner {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut EnvironmentInner {
        Arc::make_mut(&mut self.inner)
    }

    /// Validate one JavaScript binding name.
    ///
    /// Environment entries represent bindings, not member paths, so dots and
    /// other punctuation are intentionally rejected here.
    fn validated_identifier(name: impl AsRef<str>) -> Result<SmolStr, EnvironmentError> {
        let name = name.as_ref();
        let valid = !name.is_empty()
            && name.chars().enumerate().all(|(index, character)| {
                if index == 0 {
                    is_js_identifier_start(character)
                } else {
                    is_js_identifier_continue(character)
                }
            })
            && !name.is_reserved()
            && !name.is_reserved_in_strict_mode(true);
        valid
            .then_some(SmolStr::from(name))
            .ok_or_else(|| EnvironmentError { name: name.into() })
    }

    fn register_global(&mut self, name: SmolStr, object: Option<GlobalObjectMembers>) {
        let inner = self.inner_mut();
        inner.global_bindings.insert(name.clone());
        if let Some(object) = object {
            inner.global_objects.insert(name, object);
        }
    }

    /// A conservative, host-independent ECMAScript environment.
    #[must_use]
    pub fn ecmascript() -> Self {
        let global_bindings = ECMASCRIPT_GLOBALS
            .iter()
            .map(|name| SmolStr::from(*name))
            .collect();
        let global_objects = BTreeMap::from([(
            SmolStr::from("globalThis"),
            GlobalObjectMembers::ConfiguredGlobals,
        )]);
        Self {
            inner: Arc::new(EnvironmentInner {
                global_bindings,
                global_objects,
            }),
        }
    }

    /// Add a global binding supplied by the host environment.
    pub fn add_global(&mut self, name: impl AsRef<str>) -> Result<(), EnvironmentError> {
        let name = Self::validated_identifier(name)?;
        self.register_global(name, None);
        Ok(())
    }

    /// Add several host-supplied global bindings.
    pub fn add_globals<I, S>(&mut self, names: I) -> Result<(), EnvironmentError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let names = names
            .into_iter()
            .map(Self::validated_identifier)
            .collect::<Result<BTreeSet<_>, _>>()?;
        self.inner_mut().global_bindings.extend(names);
        Ok(())
    }

    /// Add a name that refers to the realm's global object.
    ///
    /// A global-object alias is also a global binding. Direct properties of
    /// this object can share callable identity with configured global bindings.
    pub fn add_global_object(&mut self, name: impl AsRef<str>) -> Result<(), EnvironmentError> {
        let name = Self::validated_identifier(name)?;
        self.register_global(name, Some(GlobalObjectMembers::ConfiguredGlobals));
        Ok(())
    }

    /// Add a global object whose promoted global identities are explicitly
    /// limited to `members`.
    ///
    /// This models a window-like object from another realm. Such an object has
    /// standard host globals but may not contain globals injected into the
    /// current plugin realm.
    pub fn add_global_object_with_members<I, S>(
        &mut self,
        name: impl AsRef<str>,
        members: I,
    ) -> Result<(), EnvironmentError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let name = Self::validated_identifier(name)?;
        let members = members
            .into_iter()
            .map(Self::validated_identifier)
            .collect::<Result<BTreeSet<_>, _>>()?;
        self.register_global(name, Some(GlobalObjectMembers::Restricted(members)));
        Ok(())
    }

    /// Merge another environment into this one. The union is additive: global
    /// bindings and objects from `other` are added; a `ConfiguredGlobals`
    /// entry in either side wins over `Restricted` for the same name.
    pub fn extend(&mut self, other: &Self) {
        let inner = self.inner_mut();
        let other_inner = other.inner();
        inner
            .global_bindings
            .extend(other_inner.global_bindings.iter().cloned());
        for (name, other_members) in &other_inner.global_objects {
            match (inner.global_objects.get_mut(name), other_members) {
                (None, _) => {
                    inner
                        .global_objects
                        .insert(name.clone(), other_members.clone());
                }
                (Some(GlobalObjectMembers::ConfiguredGlobals), _)
                | (_, GlobalObjectMembers::ConfiguredGlobals) => {
                    inner
                        .global_objects
                        .insert(name.clone(), GlobalObjectMembers::ConfiguredGlobals);
                }
                (
                    Some(GlobalObjectMembers::Restricted(members)),
                    GlobalObjectMembers::Restricted(other_members),
                ) => members.extend(other_members.iter().cloned()),
            }
        }
    }

    /// Iterate configured global binding names in deterministic order.
    pub fn global_bindings(&self) -> impl Iterator<Item = &str> {
        self.inner().global_bindings.iter().map(SmolStr::as_str)
    }

    /// Iterate configured global-object aliases in deterministic order.
    pub fn global_objects(&self) -> impl Iterator<Item = &str> {
        self.inner().global_objects.keys().map(SmolStr::as_str)
    }

    /// Whether a name is configured as a global binding.
    pub fn is_global(&self, name: &str) -> bool {
        self.inner().global_bindings.contains(name)
    }

    /// Whether a global object promotes a member to a callable identity.
    pub fn is_global_member(&self, object: &str, member: &str) -> bool {
        match self.inner().global_objects.get(object) {
            Some(GlobalObjectMembers::ConfiguredGlobals) => self.is_global(member),
            Some(GlobalObjectMembers::Restricted(members)) => members.contains(member),
            None => false,
        }
    }

    /// Whether two configured complete global-object bindings represent the
    /// same promoted realm identity. Restricted foreign-realm objects remain
    /// distinct even when their names are similar.
    pub(crate) fn global_object_aliases_match(&self, left: &str, right: &str) -> bool {
        if left == right {
            return true;
        }
        matches!(
            (
                self.inner().global_objects.get(left),
                self.inner().global_objects.get(right)
            ),
            (
                Some(GlobalObjectMembers::ConfiguredGlobals),
                Some(GlobalObjectMembers::ConfiguredGlobals)
            )
        )
    }

    pub(crate) fn global_object_name_paths_match(
        &self,
        left: &NamePath,
        right: &NamePath,
        names: &NameTable,
    ) -> bool {
        if left == right {
            return true;
        }
        let left = left.as_view();
        let right = right.as_view();

        if let (Some(left_root), Some(right_root)) = (left.first_segment(), right.first_segment())
            && names
                .resolve(*left_root)
                .zip(names.resolve(*right_root))
                .is_some_and(|(left_root, right_root)| {
                    self.global_object_aliases_match(left_root, right_root)
                })
        {
            return left.tail_after(1) == right.tail_after(1);
        }

        if self.is_promoted_global_member_path(left, names) && left.tail_after(1) == Some(right) {
            return true;
        }
        if self.is_promoted_global_member_path(right, names) && right.tail_after(1) == Some(left) {
            return true;
        }
        false
    }

    fn is_promoted_global_member_path(
        &self,
        path: PathView<'_, NameId>,
        names: &NameTable,
    ) -> bool {
        let Some(root) = path.first_segment() else {
            return false;
        };
        let Some(member) = path.tail_after(1).and_then(|tail| tail.first_segment()) else {
            return false;
        };
        let Some(root) = names.resolve(*root) else {
            return false;
        };
        let Some(member) = names.resolve(*member) else {
            return false;
        };
        self.is_global_member(root, member)
    }

    /// Hash a deterministic byte representation for cache fingerprinting
    /// directly into the running fingerprint. Iteration order follows
    /// BTreeSet/BTreeMap keys, which is stable.
    pub(crate) fn write_fingerprint_bytes(&self, fp: &mut Fingerprint) {
        self.inner().write_fingerprint_bytes(fp);
    }
}

const ECMASCRIPT_GLOBALS: &[&str] = &[
    "AggregateError",
    "Array",
    "ArrayBuffer",
    "Atomics",
    "BigInt",
    "BigInt64Array",
    "BigUint64Array",
    "Boolean",
    "DataView",
    "Date",
    "Error",
    "EvalError",
    "FinalizationRegistry",
    "Float32Array",
    "Float64Array",
    "Function",
    "Infinity",
    "Int16Array",
    "Int32Array",
    "Int8Array",
    "Intl",
    "JSON",
    "Map",
    "Math",
    "NaN",
    "Number",
    "Object",
    "Promise",
    "Proxy",
    "RangeError",
    "ReferenceError",
    "Reflect",
    "RegExp",
    "Set",
    "SharedArrayBuffer",
    "String",
    "Symbol",
    "SyntaxError",
    "TypeError",
    "URIError",
    "Uint16Array",
    "Uint32Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "WeakMap",
    "WeakRef",
    "WeakSet",
    "decodeURI",
    "decodeURIComponent",
    "encodeURI",
    "encodeURIComponent",
    "eval",
    "globalThis",
    "isFinite",
    "isNaN",
    "parseFloat",
    "parseInt",
    "undefined",
];

#[cfg(test)]
mod tests;
