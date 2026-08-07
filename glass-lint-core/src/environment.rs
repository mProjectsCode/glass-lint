//! Explicit host-environment semantics used by provenance analysis.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use glass_lint_datastructures::{Fingerprint, NamePath, NameTable, SymbolPath};
use smallvec::SmallVec;
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

struct GlobalObjectPath<'a> {
    segments: SmallVec<[&'a str; 4]>,
}

impl<'a> GlobalObjectPath<'a> {
    fn from_symbols(path: &'a SymbolPath) -> Self {
        Self {
            segments: path.segments().iter().map(SmolStr::as_str).collect(),
        }
    }

    fn from_names(path: &'a NamePath, names: &'a NameTable) -> Option<Self> {
        Some(Self {
            segments: path
                .segments()
                .iter()
                .map(|id| names.resolve(*id))
                .collect::<Option<_>>()?,
        })
    }

    fn matches(&self, environment: &Environment, other: &Self) -> bool {
        if self.segments == other.segments {
            return true;
        }

        if let (Some(left_root), Some(right_root)) = (self.segments.first(), other.segments.first())
            && environment.global_object_aliases_match(left_root, right_root)
        {
            return self.segments[1..] == other.segments[1..];
        }

        if self.is_promoted_member(environment) && &self.segments[1..] == other.segments.as_slice()
        {
            return true;
        }
        if other.is_promoted_member(environment) && &other.segments[1..] == self.segments.as_slice()
        {
            return true;
        }
        false
    }

    fn is_promoted_member(&self, environment: &Environment) -> bool {
        self.segments
            .split_first()
            .and_then(|(root, tail)| tail.first().map(|member| (root, member)))
            .is_some_and(|(root, member)| environment.is_promoted_global_member(root, member))
    }
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
    pub fn add_global(&mut self, name: impl Into<String>) -> Result<(), EnvironmentError> {
        let name = Self::validated_identifier(&name.into())?;
        self.register_global(name, None);
        Ok(())
    }

    /// Add several host-supplied global bindings.
    pub fn add_globals<I, S>(&mut self, names: I) -> Result<(), EnvironmentError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let names = names
            .into_iter()
            .map(|name| Self::validated_identifier(&name.into()))
            .collect::<Result<BTreeSet<_>, _>>()?;
        self.inner_mut().global_bindings.extend(names);
        Ok(())
    }

    /// Add a name that refers to the realm's global object.
    ///
    /// A global-object alias is also a global binding. Direct properties of
    /// this object can share callable identity with configured global bindings.
    pub fn add_global_object(&mut self, name: impl Into<String>) -> Result<(), EnvironmentError> {
        let name = Self::validated_identifier(&name.into())?;
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

    pub(crate) fn is_promoted_global_member(&self, object: &str, member: &str) -> bool {
        self.is_global_object(object) && self.is_global_member(object, member)
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

    pub(crate) fn global_object_paths_match(&self, left: &SymbolPath, right: &SymbolPath) -> bool {
        if left == right {
            return true;
        }
        GlobalObjectPath::from_symbols(left).matches(self, &GlobalObjectPath::from_symbols(right))
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
        let Some(left) = GlobalObjectPath::from_names(left, names) else {
            return false;
        };
        let Some(right) = GlobalObjectPath::from_names(right, names) else {
            return false;
        };
        left.matches(self, &right)
    }

    fn is_global_object(&self, name: &str) -> bool {
        self.inner().global_objects.contains_key(name)
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

    #[test]
    fn extend_merges_bindings_and_objects() {
        let mut base = Environment::default();
        base.add_global("alpha").unwrap();
        base.add_global_object("win1").unwrap();

        let mut other = Environment::default();
        other.add_global("beta").unwrap();
        other.add_global_object("win2").unwrap();

        base.extend(&other);
        assert!(base.is_global("alpha"));
        assert!(base.is_global("beta"));
        assert!(base.global_objects().any(|n| n == "win1"));
        assert!(base.global_objects().any(|n| n == "win2"));
    }

    #[test]
    fn extend_configured_globals_wins_over_restricted() {
        let mut base = Environment::default();
        base.add_global_object_with_members("shared", ["fetch"])
            .unwrap();

        let mut other = Environment::default();
        other.add_global_object("shared").unwrap();

        base.extend(&other);
        // After extend, "shared" becomes ConfiguredGlobals, so members
        // resolve against global bindings. "fetch" is not a default global.
        assert!(!base.is_global_member("shared", "fetch"));
        assert!(base.is_global_member("shared", "Array"));
    }

    #[test]
    fn global_object_aliases_match_configured_globals() {
        let mut env = Environment::default();
        env.add_global_object("window").unwrap();
        env.add_global_object("self").unwrap();
        env.add_global_object_with_members("foreign", ["eval"])
            .unwrap();

        assert!(env.global_object_aliases_match("window", "self"));
        assert!(!env.global_object_aliases_match("window", "foreign"));
        assert!(env.global_object_aliases_match("window", "window"));
    }

    #[test]
    fn global_object_paths_match_aliases() {
        let mut env = Environment::default();
        env.add_global("fetch").unwrap();
        env.add_global_object("window").unwrap();
        env.add_global_object("self").unwrap();

        let window_fetch = SymbolPath::from_chain("window.fetch");
        let self_fetch = SymbolPath::from_chain("self.fetch");
        assert!(env.global_object_paths_match(&window_fetch, &self_fetch));
    }

    #[test]
    fn global_object_name_paths_use_the_same_relation() {
        let mut env = Environment::default();
        env.add_global("fetch").unwrap();
        env.add_global_object("window").unwrap();
        env.add_global_object("self").unwrap();
        let mut names = NameTable::default();
        for name in ["window", "self", "fetch"] {
            names.intern(name).unwrap();
        }
        let window_fetch = names
            .lookup_path(&SymbolPath::from_chain("window.fetch"))
            .unwrap();
        let self_fetch = names
            .lookup_path(&SymbolPath::from_chain("self.fetch"))
            .unwrap();
        let fetch = names.lookup_path(&SymbolPath::from_chain("fetch")).unwrap();

        assert!(env.global_object_name_paths_match(&window_fetch, &self_fetch, &names));
        assert!(env.global_object_name_paths_match(&window_fetch, &fetch, &names));
    }

    #[test]
    fn global_object_paths_match_identical_paths() {
        let env = Environment::default();
        let path = SymbolPath::from_chain("Math");
        assert!(env.global_object_paths_match(&path, &path));
    }

    #[test]
    fn global_object_paths_match_rejects_different_paths() {
        let env = Environment::default();
        let left = SymbolPath::from_chain("Math");
        let right = SymbolPath::from_chain("JSON");
        assert!(!env.global_object_paths_match(&left, &right));
    }

    #[test]
    fn fingerprint_is_deterministic() {
        let mut a = Environment::default();
        a.add_globals(["fetch", "navigator"]).unwrap();
        let mut b = Environment::default();
        b.add_globals(["navigator", "fetch"]).unwrap();

        let mut ha = Fingerprint::init();
        let mut hb = Fingerprint::init();
        a.write_fingerprint_bytes(&mut ha);
        b.write_fingerprint_bytes(&mut hb);
        assert_eq!(ha.into_raw(), hb.into_raw());
    }

    #[test]
    fn fingerprint_differs_for_different_environments() {
        let mut a = Environment::default();
        a.add_global("fetch").unwrap();
        let b = Environment::default();

        let mut ha = Fingerprint::init();
        let mut hb = Fingerprint::init();
        a.write_fingerprint_bytes(&mut ha);
        b.write_fingerprint_bytes(&mut hb);
        assert_ne!(ha.into_raw(), hb.into_raw());
    }

    #[test]
    fn global_bindings_iterator_returns_configured_names() {
        let mut env = Environment::default();
        env.add_globals(["alpha", "beta"]).unwrap();
        let names: Vec<&str> = env.global_bindings().collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
        assert!(names.contains(&"Math"));
    }

    #[test]
    fn bulk_global_registration_is_atomic_on_validation_failure() {
        let mut env = Environment::default();
        assert!(env.add_globals(["alpha", ""]).is_err());
        assert!(!env.global_bindings().any(|name| name == "alpha"));
    }
}
