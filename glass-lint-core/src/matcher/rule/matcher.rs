#[derive(Debug, Clone, Default)]
pub struct ApiMatcher {
    pub calls: Vec<CallMatcher>,
    pub member_calls: Vec<MemberCallMatcher>,
    pub member_reads: Vec<MemberReadMatcher>,
    pub imports: Vec<String>,
    pub string_literals: Vec<String>,
    pub classes: Vec<ClassMatcher>,
    pub constructors: Vec<ConstructorMatcher>,
    pub value_flows: Vec<ValueFlowMatcher>,
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
pub enum FlowValueMatcher {
    Any,
    StaticExact(Vec<String>),
    StaticPrefix(Vec<String>),
    StaticContainsAny(Vec<String>),
    StaticContainsAll(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowCallArgMatcher {
    pub index: usize,
    pub value: FlowValueMatcher,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueFlowSource {
    pub member_call: String,
    pub arg_strings: Vec<ArgStringMatcher>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueFlowConfiguration {
    PropertyWrite {
        property: String,
        value: FlowValueMatcher,
    },
    MemberCall {
        member: String,
        args: Vec<FlowCallArgMatcher>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowSinkArgs {
    Any,
    Indices(Vec<usize>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueFlowSink {
    pub member_calls: Vec<String>,
    pub args: FlowSinkArgs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueFlowMatcher {
    pub symbol: String,
    pub sources: Vec<ValueFlowSource>,
    pub configurations: Vec<ValueFlowConfiguration>,
    pub sinks: Vec<ValueFlowSink>,
    pub all_configurations_required: bool,
}

impl ValueFlowMatcher {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            sources: Vec::new(),
            configurations: Vec::new(),
            sinks: Vec::new(),
            all_configurations_required: false,
        }
    }

    pub fn evidence_symbol(&self) -> String {
        self.symbol.clone()
    }
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
            && self.value_flows.is_empty()
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
        normalize_value_flows(&mut self.value_flows);
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

fn normalize_value_flows(values: &mut Vec<ValueFlowMatcher>) {
    for flow in values.iter_mut() {
        flow.symbol = flow.symbol.trim().to_string();
        for source in &mut flow.sources {
            source.member_call = normalize_member_chain(&source.member_call);
            for matcher in &mut source.arg_strings {
                normalize_strings(&mut matcher.values);
            }
        }
        flow.sources.retain(|source| !source.member_call.is_empty());
        flow.sources
            .sort_by(|left, right| left.member_call.cmp(&right.member_call));
        flow.sources.dedup();

        for configuration in &mut flow.configurations {
            match configuration {
                ValueFlowConfiguration::PropertyWrite { property, value } => {
                    *property = property.trim().to_string();
                    normalize_flow_value(value);
                }
                ValueFlowConfiguration::MemberCall { member, args } => {
                    *member = member.trim().to_string();
                    for arg in args.iter_mut() {
                        normalize_flow_value(&mut arg.value);
                    }
                    args.sort_by_key(|arg| arg.index);
                    args.dedup();
                }
            }
        }
        flow.configurations
            .retain(|configuration| match configuration {
                ValueFlowConfiguration::PropertyWrite { property, .. } => !property.is_empty(),
                ValueFlowConfiguration::MemberCall { member, .. } => !member.is_empty(),
            });
        flow.configurations.sort_by(|left, right| {
            configuration_sort_key(left).cmp(&configuration_sort_key(right))
        });
        flow.configurations.dedup();

        for sink in &mut flow.sinks {
            normalize_member_chains(&mut sink.member_calls);
            if let FlowSinkArgs::Indices(indices) = &mut sink.args {
                indices.sort_unstable();
                indices.dedup();
            }
        }
        flow.sinks.retain(|sink| !sink.member_calls.is_empty());
        flow.sinks.sort_by(|left, right| {
            (left.member_calls.as_slice(), sink_args_sort_key(&left.args)).cmp(&(
                right.member_calls.as_slice(),
                sink_args_sort_key(&right.args),
            ))
        });
        flow.sinks.dedup();
    }
    values.retain(|flow| {
        !flow.symbol.is_empty()
            && !flow.sources.is_empty()
            && !flow.configurations.is_empty()
            && !flow.sinks.is_empty()
    });
    values.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    values.dedup();
}

fn normalize_flow_value(value: &mut FlowValueMatcher) {
    match value {
        FlowValueMatcher::Any => {}
        FlowValueMatcher::StaticExact(values)
        | FlowValueMatcher::StaticPrefix(values)
        | FlowValueMatcher::StaticContainsAny(values)
        | FlowValueMatcher::StaticContainsAll(values) => normalize_strings(values),
    }
}

fn configuration_sort_key(configuration: &ValueFlowConfiguration) -> (&str, &str) {
    match configuration {
        ValueFlowConfiguration::PropertyWrite { property, .. } => ("property", property),
        ValueFlowConfiguration::MemberCall { member, .. } => ("member", member),
    }
}

fn sink_args_sort_key(args: &FlowSinkArgs) -> (&str, &[usize]) {
    match args {
        FlowSinkArgs::Any => ("any", &[]),
        FlowSinkArgs::Indices(indices) => ("indices", indices.as_slice()),
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
