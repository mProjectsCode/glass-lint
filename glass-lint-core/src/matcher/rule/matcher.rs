#[derive(Debug, Clone, Default)]
pub struct ApiMatcher {
    pub calls: Vec<CallMatcher>,
    pub member_calls: Vec<MemberCallMatcher>,
    pub member_reads: Vec<MemberReadMatcher>,
    pub imports: Vec<String>,
    pub string_literals: Vec<String>,
    pub classes: Vec<ClassMatcher>,
    pub constructors: Vec<ConstructorMatcher>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgStringMatcher {
    pub index: usize,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgObjectKeyMatcher {
    pub index: usize,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgRootedExprMatcher {
    pub index: usize,
    pub chains: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignedPropertyMatcher {
    pub property: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallMatcher {
    pub name: String,
    pub provenance: CallProvenance,
    pub arg_strings: Vec<ArgStringMatcher>,
}

impl CallMatcher {
    pub fn unqualified(name: String) -> Self {
        Self {
            name,
            provenance: CallProvenance::Any,
            arg_strings: Vec::new(),
        }
    }

    pub fn global(name: String) -> Self {
        Self {
            name,
            provenance: CallProvenance::Global,
            arg_strings: Vec::new(),
        }
    }

    pub fn module_export(module: String, export: String) -> Self {
        Self {
            name: export,
            provenance: CallProvenance::ModuleExport { module },
            arg_strings: Vec::new(),
        }
    }

    pub fn evidence_symbol(&self) -> String {
        match &self.provenance {
            CallProvenance::Any | CallProvenance::Global => self.name.clone(),
            CallProvenance::ModuleExport { module } => format!("{module}.{}", self.name),
        }
    }

    fn sort_key(&self) -> (&str, &str) {
        match &self.provenance {
            CallProvenance::Any => ("any", &self.name),
            CallProvenance::Global => ("global", &self.name),
            CallProvenance::ModuleExport { module } => (module, &self.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallProvenance {
    Any,
    Global,
    ModuleExport { module: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstructorMatcher {
    pub name: String,
    pub provenance: CallProvenance,
}

impl ConstructorMatcher {
    pub fn unqualified(name: String) -> Self {
        Self {
            name,
            provenance: CallProvenance::Any,
        }
    }

    pub fn global(name: String) -> Self {
        Self {
            name,
            provenance: CallProvenance::Global,
        }
    }

    pub fn module_export(module: String, export: String) -> Self {
        Self {
            name: export,
            provenance: CallProvenance::ModuleExport { module },
        }
    }

    pub fn evidence_symbol(&self) -> String {
        match &self.provenance {
            CallProvenance::Any | CallProvenance::Global => self.name.clone(),
            CallProvenance::ModuleExport { module } => format!("{module}.{}", self.name),
        }
    }

    fn sort_key(&self) -> (&str, &str) {
        match &self.provenance {
            CallProvenance::Any => ("any", &self.name),
            CallProvenance::Global => ("global", &self.name),
            CallProvenance::ModuleExport { module } => (module, &self.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberCallMatcher {
    pub chain: String,
    pub provenance: MemberCallProvenance,
    pub arg_strings: Vec<ArgStringMatcher>,
    pub arg_object_keys: Vec<ArgObjectKeyMatcher>,
    pub arg_rooted_exprs: Vec<ArgRootedExprMatcher>,
    pub assigned_properties: Vec<AssignedPropertyMatcher>,
}

impl MemberCallMatcher {
    pub fn chain(chain: String) -> Self {
        Self {
            chain,
            provenance: MemberCallProvenance::Any,
            arg_strings: Vec::new(),
            arg_object_keys: Vec::new(),
            arg_rooted_exprs: Vec::new(),
            assigned_properties: Vec::new(),
        }
    }

    pub fn rooted_chain(chain: String) -> Self {
        Self {
            chain,
            provenance: MemberCallProvenance::Rooted,
            arg_strings: Vec::new(),
            arg_object_keys: Vec::new(),
            arg_rooted_exprs: Vec::new(),
            assigned_properties: Vec::new(),
        }
    }

    pub fn module_member(module: String, member: String) -> Self {
        Self {
            chain: member,
            provenance: MemberCallProvenance::ModuleNamespace { module },
            arg_strings: Vec::new(),
            arg_object_keys: Vec::new(),
            arg_rooted_exprs: Vec::new(),
            assigned_properties: Vec::new(),
        }
    }

    pub fn evidence_symbol(&self) -> String {
        match &self.provenance {
            MemberCallProvenance::Any | MemberCallProvenance::Rooted => self.chain.clone(),
            MemberCallProvenance::ModuleNamespace { module } => format!("{module}.{}", self.chain),
        }
    }

    fn sort_key(&self) -> (&str, &str) {
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
    pub fn chain(chain: String) -> Self {
        Self {
            chain,
            provenance: MemberReadProvenance::Any,
        }
    }

    pub fn rooted_chain(chain: String) -> Self {
        Self {
            chain,
            provenance: MemberReadProvenance::Rooted,
        }
    }

    pub fn module_member(module: String, member: String) -> Self {
        Self {
            chain: member,
            provenance: MemberReadProvenance::ModuleNamespace { module },
        }
    }

    pub fn evidence_symbol(&self) -> String {
        match &self.provenance {
            MemberReadProvenance::Any | MemberReadProvenance::Rooted => self.chain.clone(),
            MemberReadProvenance::ModuleNamespace { module } => format!("{module}.{}", self.chain),
        }
    }

    fn sort_key(&self) -> (&str, &str) {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassMatcher {
    pub name: String,
    pub provenance: CallProvenance,
}

impl ClassMatcher {
    pub fn unqualified(name: String) -> Self {
        Self {
            name,
            provenance: CallProvenance::Any,
        }
    }

    pub fn module_export(module: String, export: String) -> Self {
        Self {
            name: export,
            provenance: CallProvenance::ModuleExport { module },
        }
    }

    pub fn evidence_symbol(&self) -> String {
        match &self.provenance {
            CallProvenance::Any | CallProvenance::Global => self.name.clone(),
            CallProvenance::ModuleExport { module } => format!("{module}.{}", self.name),
        }
    }

    fn sort_key(&self) -> (&str, &str) {
        match &self.provenance {
            CallProvenance::Any => ("any", &self.name),
            CallProvenance::Global => ("global", &self.name),
            CallProvenance::ModuleExport { module } => (module, &self.name),
        }
    }
}

impl ApiMatcher {
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
            && self.member_calls.is_empty()
            && self.member_reads.is_empty()
            && self.imports.is_empty()
            && self.string_literals.is_empty()
            && self.classes.is_empty()
            && self.constructors.is_empty()
    }

    pub fn normalized(mut self) -> Self {
        for call in &mut self.calls {
            call.name = call.name.trim().to_string();
            match &mut call.provenance {
                CallProvenance::Any | CallProvenance::Global => {}
                CallProvenance::ModuleExport { module } => *module = module.trim().to_string(),
            }
            for matcher in &mut call.arg_strings {
                normalize_strings(&mut matcher.values);
            }
        }
        self.calls.retain(|call| {
            !call.name.is_empty()
                && match &call.provenance {
                    CallProvenance::Any | CallProvenance::Global => true,
                    CallProvenance::ModuleExport { module } => !module.is_empty(),
                }
        });
        self.calls
            .sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        self.calls.dedup();

        for member_call in &mut self.member_calls {
            member_call.chain = normalize_member_chain(&member_call.chain);
            if member_call.provenance == MemberCallProvenance::Rooted {
                member_call.chain = canonical_rooted_chain(&member_call.chain).to_string();
            }
            if let MemberCallProvenance::ModuleNamespace { module } = &mut member_call.provenance {
                *module = module.trim().to_string();
            }
        }
        self.member_calls.retain(|call| {
            !call.chain.is_empty()
                && match &call.provenance {
                    MemberCallProvenance::Any | MemberCallProvenance::Rooted => true,
                    MemberCallProvenance::ModuleNamespace { module } => !module.is_empty(),
                }
        });
        self.member_calls
            .sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        self.member_calls.dedup();

        for member_read in &mut self.member_reads {
            member_read.chain = normalize_member_chain(&member_read.chain);
            if member_read.provenance == MemberReadProvenance::Rooted {
                member_read.chain = canonical_rooted_chain(&member_read.chain).to_string();
            }
            if let MemberReadProvenance::ModuleNamespace { module } = &mut member_read.provenance {
                *module = module.trim().to_string();
            }
        }
        self.member_reads.retain(|read| {
            !read.chain.is_empty()
                && match &read.provenance {
                    MemberReadProvenance::Any | MemberReadProvenance::Rooted => true,
                    MemberReadProvenance::ModuleNamespace { module } => !module.is_empty(),
                }
        });
        self.member_reads
            .sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        self.member_reads.dedup();
        normalize_strings(&mut self.imports);
        normalize_strings(&mut self.string_literals);
        normalize_class_matchers(&mut self.classes);
        normalize_constructor_matchers(&mut self.constructors);
        for member_call in &mut self.member_calls {
            for matcher in &mut member_call.arg_strings {
                normalize_strings(&mut matcher.values);
            }
            for matcher in &mut member_call.arg_object_keys {
                normalize_strings(&mut matcher.keys);
            }
            for matcher in &mut member_call.arg_rooted_exprs {
                normalize_member_chains(&mut matcher.chains);
            }
            for matcher in &mut member_call.assigned_properties {
                matcher.property = matcher.property.trim().to_string();
                normalize_strings(&mut matcher.values);
            }
            member_call
                .assigned_properties
                .retain(|matcher| !matcher.property.is_empty());
        }
        self
    }
}

fn normalize_call_provenance(provenance: &mut CallProvenance) {
    if let CallProvenance::ModuleExport { module } = provenance {
        *module = module.trim().to_string();
    }
}

fn normalize_class_matchers(values: &mut Vec<ClassMatcher>) {
    for value in values.iter_mut() {
        value.name = value.name.trim().to_string();
        normalize_call_provenance(&mut value.provenance);
    }
    values.retain(|value| {
        !value.name.is_empty()
            && match &value.provenance {
                CallProvenance::Any | CallProvenance::Global => true,
                CallProvenance::ModuleExport { module } => !module.is_empty(),
            }
    });
    values.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    values.dedup();
}

fn normalize_constructor_matchers(values: &mut Vec<ConstructorMatcher>) {
    for value in values.iter_mut() {
        value.name = value.name.trim().to_string();
        normalize_call_provenance(&mut value.provenance);
    }
    values.retain(|value| {
        !value.name.is_empty()
            && match &value.provenance {
                CallProvenance::Any | CallProvenance::Global => true,
                CallProvenance::ModuleExport { module } => !module.is_empty(),
            }
    });
    values.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    values.dedup();
}

fn normalize_strings(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    for value in values.iter_mut() {
        *value = value.trim().to_string();
    }
    values.sort();
    values.dedup();
}

fn normalize_member_chains(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    for value in values.iter_mut() {
        *value = normalize_member_chain(value);
        *value = canonical_rooted_chain(value).to_string();
    }
    values.retain(|value| !value.is_empty());
    values.sort();
    values.dedup();
}

fn normalize_member_chain(value: &str) -> String {
    value
        .split('.')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(".")
}

pub fn canonical_rooted_chain(value: &str) -> &str {
    value.strip_prefix("this.").unwrap_or(value)
}
