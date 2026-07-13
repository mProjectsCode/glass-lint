use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberCallMatcher {
    pub chain: String,
    pub provenance: MemberCallProvenance,
    pub arg_strings: Vec<ArgStringMatcher>,
    pub arg_object_keys: Vec<ArgObjectKeyMatcher>,
    pub arg_rooted_exprs: Vec<ArgRootedExprMatcher>,
}

impl MemberCallMatcher {
    pub fn syntactic_heuristic(chain: impl Into<String>) -> Self {
        Self {
            chain: chain.into(),
            provenance: MemberCallProvenance::Any,
            arg_strings: Vec::new(),
            arg_object_keys: Vec::new(),
            arg_rooted_exprs: Vec::new(),
        }
    }

    pub fn rooted_chain(chain: impl Into<String>) -> Self {
        Self {
            chain: chain.into(),
            provenance: MemberCallProvenance::Rooted,
            arg_strings: Vec::new(),
            arg_object_keys: Vec::new(),
            arg_rooted_exprs: Vec::new(),
        }
    }

    pub fn module_member(module: impl Into<String>, member: impl Into<String>) -> Self {
        Self {
            chain: member.into(),
            provenance: MemberCallProvenance::ModuleNamespace {
                module: module.into(),
            },
            arg_strings: Vec::new(),
            arg_object_keys: Vec::new(),
            arg_rooted_exprs: Vec::new(),
        }
    }

    pub fn arg_string<I, S>(mut self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arg_strings.push(ArgStringMatcher {
            index,
            values: values.into_iter().map(Into::into).collect(),
            predicate: None,
        });
        self
    }

    pub fn static_string_arg(mut self, index: usize) -> Self {
        self.arg_strings.push(ArgStringMatcher {
            index,
            values: Vec::new(),
            predicate: None,
        });
        self
    }

    pub fn arg_value(mut self, index: usize, value: FlowValueMatcher) -> Self {
        self.arg_strings.push(ArgStringMatcher {
            index,
            values: Vec::new(),
            predicate: Some(value),
        });
        self
    }

    pub fn arg_object_keys<I, S>(mut self, index: usize, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arg_object_keys.push(ArgObjectKeyMatcher {
            index,
            keys: keys.into_iter().map(Into::into).collect(),
        });
        self
    }

    pub fn arg_rooted_exprs<I, S>(mut self, index: usize, chains: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arg_rooted_exprs.push(ArgRootedExprMatcher {
            index,
            chains: chains.into_iter().map(Into::into).collect(),
        });
        self
    }

    pub fn evidence_symbol(&self) -> String {
        match &self.provenance {
            MemberCallProvenance::Any | MemberCallProvenance::Rooted => self.chain.clone(),
            MemberCallProvenance::ModuleNamespace { module } => format!("{module}.{}", self.chain),
        }
    }

    pub(crate) fn sort_key(&self) -> (&str, &str) {
        match &self.provenance {
            MemberCallProvenance::Any => ("any", &self.chain),
            MemberCallProvenance::Rooted => ("rooted", &self.chain),
            MemberCallProvenance::ModuleNamespace { module } => (module, &self.chain),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberCallProvenance {
    Any,
    Rooted,
    ModuleNamespace { module: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberReadMatcher {
    pub chain: String,
    pub provenance: MemberReadProvenance,
}

impl MemberReadMatcher {
    pub fn syntactic_heuristic(chain: impl Into<String>) -> Self {
        Self {
            chain: chain.into(),
            provenance: MemberReadProvenance::Any,
        }
    }

    pub fn rooted_chain(chain: impl Into<String>) -> Self {
        Self {
            chain: chain.into(),
            provenance: MemberReadProvenance::Rooted,
        }
    }

    pub fn module_member(module: impl Into<String>, member: impl Into<String>) -> Self {
        Self {
            chain: member.into(),
            provenance: MemberReadProvenance::ModuleNamespace {
                module: module.into(),
            },
        }
    }

    pub fn evidence_symbol(&self) -> String {
        match &self.provenance {
            MemberReadProvenance::Any | MemberReadProvenance::Rooted => self.chain.clone(),
            MemberReadProvenance::ModuleNamespace { module } => format!("{module}.{}", self.chain),
        }
    }

    pub(crate) fn sort_key(&self) -> (&str, &str) {
        match &self.provenance {
            MemberReadProvenance::Any => ("any", &self.chain),
            MemberReadProvenance::Rooted => ("rooted", &self.chain),
            MemberReadProvenance::ModuleNamespace { module } => (module, &self.chain),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberReadProvenance {
    Any,
    Rooted,
    ModuleNamespace { module: String },
}
