//! Member-call and member-read matcher declarations.

use super::{ArgumentConstraint, ArgumentMatcher};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Matcher for a member call with optional argument predicates.
pub struct MemberCallMatcher {
    /// Static member chain.
    pub chain: String,
    /// Required rooted/module provenance mode.
    pub provenance: MemberCallProvenance,
    /// Predicates attached to zero-based argument positions.
    pub arguments: Vec<ArgumentConstraint>,
}

impl MemberCallMatcher {
    /// Construct a spelling-based heuristic member matcher.
    pub fn heuristic(chain: impl Into<String>) -> Self {
        Self::new(chain, MemberCallProvenance::Any)
    }

    /// Construct a matcher requiring a rooted identity.
    pub fn rooted(chain: impl Into<String>) -> Self {
        Self::new(chain, MemberCallProvenance::Rooted)
    }

    /// Construct a matcher for a member of an imported module namespace.
    pub fn module_member(module: impl Into<String>, member: impl Into<String>) -> Self {
        Self::new(
            member,
            MemberCallProvenance::ModuleNamespace {
                module: module.into(),
            },
        )
    }

    fn new(chain: impl Into<String>, provenance: MemberCallProvenance) -> Self {
        Self {
            chain: chain.into(),
            provenance,
            arguments: Vec::new(),
        }
    }

    #[must_use]
    /// Add a predicate for one argument position.
    pub fn arg(mut self, index: usize, matcher: impl Into<ArgumentMatcher>) -> Self {
        self.arguments.push(ArgumentConstraint {
            index,
            matcher: matcher.into(),
        });
        self
    }

    pub fn syntactic_heuristic(chain: impl Into<String>) -> Self {
        Self::heuristic(chain)
    }

    pub fn rooted_chain(chain: impl Into<String>) -> Self {
        Self::rooted(chain)
    }

    #[must_use]
    pub fn static_string_arg(self, index: usize) -> Self {
        self.arg(index, super::ValueMatcher::static_string())
    }

    #[must_use]
    pub fn arg_string<I, S>(self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arg(
            index,
            super::ValueMatcher::static_string().equals_any(values),
        )
    }

    #[must_use]
    pub fn arg_value(self, index: usize, value: impl Into<super::ValueMatcher>) -> Self {
        self.arg(index, value.into())
    }

    #[must_use]
    pub fn arg_object_keys<I, S>(self, index: usize, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arg(index, super::ArgumentMatcher::object_keys(keys))
    }

    #[must_use]
    pub fn arg_rooted_exprs<I, S>(self, index: usize, chains: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arg(index, super::ArgumentMatcher::rooted_expressions(chains))
    }

    /// Return the display/evidence symbol for this matcher.
    pub fn evidence_symbol(&self) -> String {
        match &self.provenance {
            MemberCallProvenance::Any | MemberCallProvenance::Rooted => self.chain.clone(),
            MemberCallProvenance::ModuleNamespace { module } => format!("{module}.{}", self.chain),
        }
    }

    /// Return the deterministic normalization sort key.
    pub fn sort_key(&self) -> (&str, &str) {
        match &self.provenance {
            MemberCallProvenance::Any => ("any", &self.chain),
            MemberCallProvenance::Rooted => ("rooted", &self.chain),
            MemberCallProvenance::ModuleNamespace { module } => (module, &self.chain),
        }
    }

    /// Borrow the member chain.
    pub fn chain(&self) -> &str {
        &self.chain
    }

    /// Borrow argument predicates.
    pub fn arguments(&self) -> &[ArgumentConstraint] {
        &self.arguments
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Provenance requirement for a member call.
pub enum MemberCallProvenance {
    /// Accept any member spelling/provenance.
    Any,
    /// Require a rooted identity.
    Rooted,
    /// Require a member of an imported module namespace.
    ModuleNamespace { module: String },
}

impl MemberCallProvenance {
    /// Test whether this mode accepts a rooted occurrence.
    pub fn matches_rooted(&self, rooted: bool) -> bool {
        match self {
            Self::Any => true,
            Self::Rooted => rooted,
            Self::ModuleNamespace { .. } => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Matcher for a non-call member read.
pub struct MemberReadMatcher {
    /// Static member chain.
    pub chain: String,
    /// Required provenance mode.
    pub provenance: MemberReadProvenance,
}

impl MemberReadMatcher {
    /// Construct a spelling-based heuristic member-read matcher.
    pub fn heuristic(chain: impl Into<String>) -> Self {
        Self {
            chain: chain.into(),
            provenance: MemberReadProvenance::Any,
        }
    }

    /// Construct a matcher requiring a rooted identity.
    pub fn rooted(chain: impl Into<String>) -> Self {
        Self {
            chain: chain.into(),
            provenance: MemberReadProvenance::Rooted,
        }
    }

    /// Construct a matcher for a member of an imported module namespace.
    pub fn module_member(module: impl Into<String>, member: impl Into<String>) -> Self {
        Self {
            chain: member.into(),
            provenance: MemberReadProvenance::ModuleNamespace {
                module: module.into(),
            },
        }
    }

    /// Return the display/evidence symbol for this matcher.
    pub fn evidence_symbol(&self) -> String {
        match &self.provenance {
            MemberReadProvenance::Any | MemberReadProvenance::Rooted => self.chain.clone(),
            MemberReadProvenance::ModuleNamespace { module } => format!("{module}.{}", self.chain),
        }
    }

    /// Return the deterministic normalization sort key.
    pub fn sort_key(&self) -> (&str, &str) {
        match &self.provenance {
            MemberReadProvenance::Any => ("any", &self.chain),
            MemberReadProvenance::Rooted => ("rooted", &self.chain),
            MemberReadProvenance::ModuleNamespace { module } => (module, &self.chain),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Provenance requirement for a member read.
pub enum MemberReadProvenance {
    /// Accept any member spelling/provenance.
    Any,
    /// Require a rooted identity.
    Rooted,
    /// Require a member of an imported module namespace.
    ModuleNamespace { module: String },
}
