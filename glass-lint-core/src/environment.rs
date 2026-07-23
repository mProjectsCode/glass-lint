//! Explicit host-environment semantics used by provenance analysis.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use smol_str::SmolStr;

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

#[derive(Clone, Debug, PartialEq)]
struct EnvironmentInner {
    global_bindings: BTreeSet<SmolStr>,
    global_objects: BTreeMap<SmolStr, GlobalObjectMembers>,
}

#[derive(Clone, Debug, PartialEq)]
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
    c == '$' || c == '_' || c.is_ascii_alphabetic()
}

fn is_js_identifier_continue(c: char) -> bool {
    c == '$' || c == '_' || c.is_ascii_alphanumeric()
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
    fn validated_identifier(name: &str) -> Result<SmolStr, EnvironmentError> {
        let valid = !name.is_empty()
            && name.chars().enumerate().all(|(index, character)| {
                if index == 0 {
                    is_js_identifier_start(character)
                } else {
                    is_js_identifier_continue(character)
                }
            });
        valid
            .then_some(SmolStr::from(name))
            .ok_or_else(|| EnvironmentError { name: name.into() })
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
    pub fn add_global(&mut self, name: impl Into<String>) -> Result<(), EnvironmentError> {
        let name = Self::validated_identifier(&name.into())?;
        self.inner_mut().global_bindings.insert(name);
        Ok(())
    }

    /// Add several host-supplied global bindings.
    pub fn add_globals<I, S>(&mut self, names: I) -> Result<(), EnvironmentError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for name in names {
            self.add_global(name)?;
        }
        Ok(())
    }

    /// Add a name that refers to the realm's global object.
    ///
    /// A global-object alias is also a global binding. Direct properties of
    /// this object can share callable identity with configured global bindings.
    pub fn add_global_object(&mut self, name: impl Into<String>) -> Result<(), EnvironmentError> {
        let name = Self::validated_identifier(&name.into())?;
        let inner = self.inner_mut();
        inner.global_bindings.insert(name.clone());
        inner
            .global_objects
            .insert(name, GlobalObjectMembers::ConfiguredGlobals);
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
        name: impl Into<String>,
        members: I,
    ) -> Result<(), EnvironmentError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let name = Self::validated_identifier(&name.into())?;
        let members = members
            .into_iter()
            .map(|member| Self::validated_identifier(&member.into()))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let inner = self.inner_mut();
        inner.global_bindings.insert(name.clone());
        inner
            .global_objects
            .insert(name, GlobalObjectMembers::Restricted(members));
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

    pub(crate) fn global_object_paths_match(&self, left: &[SmolStr], right: &[SmolStr]) -> bool {
        if left == right {
            return true;
        }
        if let (Some(left_root), Some(right_root)) = (left.first(), right.first())
            && self.global_object_aliases_match(left_root, right_root)
        {
            return left[1..] == right[1..];
        }
        if let Some(root) = left.first()
            && self.is_global_object(root)
            && left.len() > 1
            && self.is_global_member(root, &left[1])
            && &left[1..] == right
        {
            return true;
        }
        if let Some(root) = right.first()
            && self.is_global_object(root)
            && right.len() > 1
            && self.is_global_member(root, &right[1])
            && &right[1..] == left
        {
            return true;
        }
        false
    }

    fn is_global_object(&self, name: &str) -> bool {
        self.inner().global_objects.contains_key(name)
    }

    /// Hash a deterministic byte representation for cache fingerprinting
    /// directly into the FNV-1a state. Iteration order follows
    /// BTreeSet/BTreeMap keys, which is stable.
    pub(crate) fn write_fingerprint_bytes(&self, h: &mut u64) {
        fn write(h: &mut u64, bytes: &[u8]) {
            for &b in bytes {
                *h ^= u64::from(b);
                *h = h.wrapping_mul(0x100_0000_01b3);
            }
        }
        let inner = self.inner();
        // Global bindings (sorted).
        write(h, &(inner.global_bindings.len() as u64).to_le_bytes());
        for name in &inner.global_bindings {
            write(h, name.as_bytes());
            write(h, &[0u8]);
        }
        // Global objects (sorted by name).
        write(h, &(inner.global_objects.len() as u64).to_le_bytes());
        for (name, members) in &inner.global_objects {
            write(h, name.as_bytes());
            write(h, &[0u8]);
            match members {
                GlobalObjectMembers::ConfiguredGlobals => {
                    write(h, &[0u8]);
                }
                GlobalObjectMembers::Restricted(member_set) => {
                    write(h, &[1u8]);
                    write(h, &(member_set.len() as u64).to_le_bytes());
                    for member in member_set {
                        write(h, member.as_bytes());
                        write(h, &[0u8]);
                    }
                }
            }
        }
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
mod tests {
    use super::*;

    #[test]
    fn defaults_are_host_independent_and_extensions_are_additive() {
        let mut environment = Environment::default();
        assert!(environment.is_global("Math"));
        assert!(
            environment
                .global_objects()
                .any(|name| name == "globalThis")
        );
        assert!(!environment.is_global("fetch"));
        assert!(!environment.global_objects().any(|name| name == "window"));

        environment.add_global("fetch").unwrap();
        environment.add_global_object("activeWindow").unwrap();
        assert!(environment.is_global("fetch"));
        assert!(environment.is_global("activeWindow"));
        assert!(
            environment
                .global_objects()
                .any(|name| name == "activeWindow")
        );
    }

    #[test]
    fn restricted_global_objects_do_not_inherit_current_realm_injections() {
        let mut environment = Environment::default();
        environment.add_global("requestUrl").unwrap();
        environment
            .add_global_object_with_members("activeWindow", ["eval", "fetch"])
            .unwrap();

        assert!(environment.is_global_member("activeWindow", "eval"));
        assert!(environment.is_global_member("activeWindow", "fetch"));
        assert!(!environment.is_global_member("activeWindow", "requestUrl"));
        assert!(environment.is_global_member("globalThis", "requestUrl"));
    }

    #[test]
    fn rejects_paths_and_other_non_identifiers() {
        let mut environment = Environment::default();
        assert!(environment.add_global("window.fetch").is_err());
        assert!(environment.add_global_object("").is_err());
    }
}
