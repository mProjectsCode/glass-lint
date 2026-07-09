#[derive(Debug, Clone, Default)]
pub struct ApiMatcher {
    pub calls: Vec<CallMatcher>,
    pub member_calls: Vec<MemberCallMatcher>,
    pub member_reads: Vec<MemberReadMatcher>,
    pub imports: Vec<String>,
    pub string_literals: Vec<String>,
    pub classes: Vec<ClassMatcher>,
    pub constructors: Vec<ConstructorMatcher>,
    pub flows: Vec<FlowMatcher>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Matcher {
    Call(CallMatcher),
    MemberCall(MemberCallMatcher),
    MemberRead(MemberReadMatcher),
    Import(String),
    StringLiteral(String),
    Class(ClassMatcher),
    Constructor(ConstructorMatcher),
    Flow(FlowMatcher),
}

impl Matcher {
    pub fn call(value: CallMatcher) -> Self {
        Self::Call(value)
    }

    pub fn heuristic_call(name: impl Into<String>) -> Self {
        Self::Call(CallMatcher::heuristic(name))
    }

    pub fn global_call(name: impl Into<String>) -> Self {
        Self::Call(CallMatcher::global(name))
    }

    pub fn module_call(module: impl Into<String>, export: impl Into<String>) -> Self {
        Self::Call(CallMatcher::module_export(module, export))
    }

    pub fn member_call(value: MemberCallMatcher) -> Self {
        Self::MemberCall(value)
    }

    pub fn heuristic_member_call(chain: impl Into<String>) -> Self {
        Self::MemberCall(MemberCallMatcher::syntactic_heuristic(chain))
    }

    pub fn rooted_member_call(chain: impl Into<String>) -> Self {
        Self::MemberCall(MemberCallMatcher::rooted_chain(chain))
    }

    pub fn module_member_call(module: impl Into<String>, member: impl Into<String>) -> Self {
        Self::MemberCall(MemberCallMatcher::module_member(module, member))
    }

    pub fn member_read(value: MemberReadMatcher) -> Self {
        Self::MemberRead(value)
    }

    pub fn heuristic_member_read(chain: impl Into<String>) -> Self {
        Self::MemberRead(MemberReadMatcher::syntactic_heuristic(chain))
    }

    pub fn rooted_member_read(chain: impl Into<String>) -> Self {
        Self::MemberRead(MemberReadMatcher::rooted_chain(chain))
    }

    pub fn module_member_read(module: impl Into<String>, member: impl Into<String>) -> Self {
        Self::MemberRead(MemberReadMatcher::module_member(module, member))
    }

    pub fn import(module: impl Into<String>) -> Self {
        Self::Import(module.into())
    }

    pub fn string_literal(value: impl Into<String>) -> Self {
        Self::StringLiteral(value.into())
    }

    pub fn class(value: ClassMatcher) -> Self {
        Self::Class(value)
    }

    pub fn heuristic_class(name: impl Into<String>) -> Self {
        Self::Class(ClassMatcher::heuristic(name))
    }

    pub fn module_class(module: impl Into<String>, export: impl Into<String>) -> Self {
        Self::Class(ClassMatcher::module_export(module, export))
    }

    pub fn constructor(value: ConstructorMatcher) -> Self {
        Self::Constructor(value)
    }

    pub fn heuristic_constructor(name: impl Into<String>) -> Self {
        Self::Constructor(ConstructorMatcher::heuristic(name))
    }

    pub fn global_constructor(name: impl Into<String>) -> Self {
        Self::Constructor(ConstructorMatcher::global(name))
    }

    pub fn module_constructor(module: impl Into<String>, export: impl Into<String>) -> Self {
        Self::Constructor(ConstructorMatcher::module_export(module, export))
    }

    pub fn flow(value: FlowMatcher) -> Self {
        Self::Flow(value)
    }
}

impl From<CallMatcher> for Matcher {
    fn from(value: CallMatcher) -> Self {
        Self::Call(value)
    }
}

impl From<MemberCallMatcher> for Matcher {
    fn from(value: MemberCallMatcher) -> Self {
        Self::MemberCall(value)
    }
}

impl From<MemberReadMatcher> for Matcher {
    fn from(value: MemberReadMatcher) -> Self {
        Self::MemberRead(value)
    }
}

impl From<ClassMatcher> for Matcher {
    fn from(value: ClassMatcher) -> Self {
        Self::Class(value)
    }
}

impl From<ConstructorMatcher> for Matcher {
    fn from(value: ConstructorMatcher) -> Self {
        Self::Constructor(value)
    }
}

impl From<FlowMatcher> for Matcher {
    fn from(value: FlowMatcher) -> Self {
        Self::Flow(value)
    }
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
pub struct FlowSource {
    pub member_call: String,
    pub arg_strings: Vec<ArgStringMatcher>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowRequirement {
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
pub struct FlowSink {
    pub member_calls: Vec<String>,
    pub args: FlowSinkArgs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowMatcher {
    pub symbol: String,
    pub sources: Vec<FlowSource>,
    pub requirements: Vec<FlowRequirement>,
    pub sinks: Vec<FlowSink>,
    pub all_requirements_required: bool,
    pub emit_on_requirements: bool,
}

impl FlowMatcher {
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            sources: Vec::new(),
            requirements: Vec::new(),
            sinks: Vec::new(),
            all_requirements_required: false,
            emit_on_requirements: false,
        }
    }

    pub fn source_member_call(mut self, member_call: impl Into<String>) -> Self {
        self.sources.push(FlowSource {
            member_call: member_call.into(),
            arg_strings: Vec::new(),
        });
        self
    }

    pub fn source_arg_string<I, S>(mut self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let Some(source) = self.sources.last_mut() else {
            return self;
        };
        source.arg_strings.push(ArgStringMatcher {
            index,
            values: values.into_iter().map(Into::into).collect(),
        });
        self
    }

    pub fn property_write(mut self, property: impl Into<String>, value: FlowValueMatcher) -> Self {
        self.requirements.push(FlowRequirement::PropertyWrite {
            property: property.into(),
            value,
        });
        self
    }

    pub fn member_call_config<I>(mut self, member: impl Into<String>, args: I) -> Self
    where
        I: IntoIterator<Item = (usize, FlowValueMatcher)>,
    {
        self.requirements.push(FlowRequirement::MemberCall {
            member: member.into(),
            args: args
                .into_iter()
                .map(|(index, value)| FlowCallArgMatcher { index, value })
                .collect(),
        });
        self
    }

    pub fn require_all(mut self) -> Self {
        self.all_requirements_required = true;
        self
    }

    pub fn emit_when_requirements_met(mut self) -> Self {
        self.emit_on_requirements = true;
        self
    }

    pub fn sink_member_call_arg_indices<I, S, J>(mut self, member_calls: I, indices: J) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
        J: IntoIterator<Item = usize>,
    {
        self.sinks.push(FlowSink {
            member_calls: member_calls.into_iter().map(Into::into).collect(),
            args: FlowSinkArgs::Indices(indices.into_iter().collect()),
        });
        self
    }

    pub fn sink_member_call_any_arg<I, S>(mut self, member_calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.sinks.push(FlowSink {
            member_calls: member_calls.into_iter().map(Into::into).collect(),
            args: FlowSinkArgs::Any,
        });
        self
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
    pub fn heuristic(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provenance: CallProvenance::Any,
            arg_strings: Vec::new(),
        }
    }

    pub fn global(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provenance: CallProvenance::Global,
            arg_strings: Vec::new(),
        }
    }

    pub fn module_export(module: impl Into<String>, export: impl Into<String>) -> Self {
        Self {
            name: export.into(),
            provenance: CallProvenance::ModuleExport {
                module: module.into(),
            },
            arg_strings: Vec::new(),
        }
    }

    pub fn static_string_arg(mut self, index: usize) -> Self {
        self.arg_strings.push(ArgStringMatcher {
            index,
            values: Vec::new(),
        });
        self
    }

    pub fn arg_string<I, S>(mut self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arg_strings.push(ArgStringMatcher {
            index,
            values: values.into_iter().map(Into::into).collect(),
        });
        self
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
    pub fn heuristic(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provenance: CallProvenance::Any,
        }
    }

    pub fn global(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provenance: CallProvenance::Global,
        }
    }

    pub fn module_export(module: impl Into<String>, export: impl Into<String>) -> Self {
        Self {
            name: export.into(),
            provenance: CallProvenance::ModuleExport {
                module: module.into(),
            },
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
        });
        self
    }

    pub fn static_string_arg(mut self, index: usize) -> Self {
        self.arg_strings.push(ArgStringMatcher {
            index,
            values: Vec::new(),
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
    pub fn heuristic(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provenance: CallProvenance::Any,
        }
    }

    pub fn module_export(module: impl Into<String>, export: impl Into<String>) -> Self {
        Self {
            name: export.into(),
            provenance: CallProvenance::ModuleExport {
                module: module.into(),
            },
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
    pub fn from_matchers(matchers: Vec<Matcher>) -> Self {
        let mut api_matcher = Self::default();
        for matcher in matchers {
            api_matcher.push(matcher);
        }
        api_matcher
    }

    pub fn into_matchers(self) -> Vec<Matcher> {
        self.calls
            .into_iter()
            .map(Matcher::Call)
            .chain(self.member_calls.into_iter().map(Matcher::MemberCall))
            .chain(self.member_reads.into_iter().map(Matcher::MemberRead))
            .chain(self.imports.into_iter().map(Matcher::Import))
            .chain(self.string_literals.into_iter().map(Matcher::StringLiteral))
            .chain(self.classes.into_iter().map(Matcher::Class))
            .chain(self.constructors.into_iter().map(Matcher::Constructor))
            .chain(self.flows.into_iter().map(Matcher::Flow))
            .collect()
    }

    pub fn push(&mut self, matcher: Matcher) {
        match matcher {
            Matcher::Call(value) => self.calls.push(value),
            Matcher::MemberCall(value) => self.member_calls.push(value),
            Matcher::MemberRead(value) => self.member_reads.push(value),
            Matcher::Import(value) => self.imports.push(value),
            Matcher::StringLiteral(value) => self.string_literals.push(value),
            Matcher::Class(value) => self.classes.push(value),
            Matcher::Constructor(value) => self.constructors.push(value),
            Matcher::Flow(value) => self.flows.push(value),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
            && self.member_calls.is_empty()
            && self.member_reads.is_empty()
            && self.imports.is_empty()
            && self.string_literals.is_empty()
            && self.classes.is_empty()
            && self.constructors.is_empty()
            && self.flows.is_empty()
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
        normalize_flows(&mut self.flows);
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
        }
        self
    }
}

fn normalize_flows(values: &mut Vec<FlowMatcher>) {
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

        for requirement in &mut flow.requirements {
            match requirement {
                FlowRequirement::PropertyWrite { property, value } => {
                    *property = property.trim().to_string();
                    normalize_flow_value(value);
                }
                FlowRequirement::MemberCall { member, args } => {
                    *member = member.trim().to_string();
                    for arg in args.iter_mut() {
                        normalize_flow_value(&mut arg.value);
                    }
                    args.sort_by_key(|arg| arg.index);
                    args.dedup();
                }
            }
        }
        flow.requirements.retain(|requirement| match requirement {
            FlowRequirement::PropertyWrite { property, .. } => !property.is_empty(),
            FlowRequirement::MemberCall { member, .. } => !member.is_empty(),
        });
        flow.requirements
            .sort_by(|left, right| requirement_sort_key(left).cmp(&requirement_sort_key(right)));
        flow.requirements.dedup();

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
            && !flow.requirements.is_empty()
            && (flow.emit_on_requirements || !flow.sinks.is_empty())
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

fn requirement_sort_key(requirement: &FlowRequirement) -> (&str, &str) {
    match requirement {
        FlowRequirement::PropertyWrite { property, .. } => ("property", property),
        FlowRequirement::MemberCall { member, .. } => ("member", member),
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
