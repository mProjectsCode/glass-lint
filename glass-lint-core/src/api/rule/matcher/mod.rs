mod call;
mod derived;
mod flow;
mod member;

pub use call::*;
pub use derived::*;
pub use flow::*;
pub use member::*;

#[derive(Debug, Clone, Default)]
pub struct ApiMatcher {
    pub(crate) calls: Vec<CallMatcher>,
    pub(crate) member_calls: Vec<MemberCallMatcher>,
    pub(crate) member_reads: Vec<MemberReadMatcher>,
    pub(crate) imports: Vec<String>,
    pub(crate) string_literals: Vec<String>,
    pub(crate) classes: Vec<ClassMatcher>,
    pub(crate) constructors: Vec<ConstructorMatcher>,
    pub(crate) flows: Vec<FlowMatcher>,
    pub(crate) returned_member_calls: Vec<ReturnedMemberCallMatcher>,
    pub(crate) returned_member_reads: Vec<ReturnedMemberReadMatcher>,
    pub(crate) instance_member_calls: Vec<InstanceMemberCallMatcher>,
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
    ReturnedMemberCall(ReturnedMemberCallMatcher),
    ReturnedMemberRead(ReturnedMemberReadMatcher),
    InstanceMemberCall(InstanceMemberCallMatcher),
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

    pub fn returned_member_call(source: impl Into<String>, member: impl Into<String>) -> Self {
        Self::ReturnedMemberCall(ReturnedMemberCallMatcher {
            source: source.into(),
            member: member.into(),
        })
    }

    pub fn returned_member_read(source: impl Into<String>, member: impl Into<String>) -> Self {
        Self::ReturnedMemberRead(ReturnedMemberReadMatcher {
            source: source.into(),
            member: member.into(),
        })
    }

    pub fn instance_member_call(
        module: impl Into<String>,
        export: impl Into<String>,
        member: impl Into<String>,
    ) -> Self {
        Self::InstanceMemberCall(InstanceMemberCallMatcher {
            module: module.into(),
            export: export.into(),
            member: member.into(),
        })
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

impl From<ReturnedMemberCallMatcher> for Matcher {
    fn from(value: ReturnedMemberCallMatcher) -> Self {
        Self::ReturnedMemberCall(value)
    }
}

impl From<ReturnedMemberReadMatcher> for Matcher {
    fn from(value: ReturnedMemberReadMatcher) -> Self {
        Self::ReturnedMemberRead(value)
    }
}

impl From<InstanceMemberCallMatcher> for Matcher {
    fn from(value: InstanceMemberCallMatcher) -> Self {
        Self::InstanceMemberCall(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgStringMatcher {
    pub index: usize,
    pub values: Vec<String>,
    pub predicate: Option<FlowValueMatcher>,
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

impl ApiMatcher {
    pub(crate) fn validate(&self) -> Result<(), String> {
        super::validation::validate(self)
    }

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
            .chain(
                self.returned_member_calls
                    .into_iter()
                    .map(Matcher::ReturnedMemberCall),
            )
            .chain(
                self.returned_member_reads
                    .into_iter()
                    .map(Matcher::ReturnedMemberRead),
            )
            .chain(
                self.instance_member_calls
                    .into_iter()
                    .map(Matcher::InstanceMemberCall),
            )
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
            Matcher::ReturnedMemberCall(value) => self.returned_member_calls.push(value),
            Matcher::ReturnedMemberRead(value) => self.returned_member_reads.push(value),
            Matcher::InstanceMemberCall(value) => self.instance_member_calls.push(value),
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
            && self.returned_member_calls.is_empty()
            && self.returned_member_reads.is_empty()
            && self.instance_member_calls.is_empty()
    }

    pub fn normalized(self) -> Self {
        super::normalization::normalize(self)
    }
}

pub(super) fn normalize_returned_member_calls(values: &mut Vec<ReturnedMemberCallMatcher>) {
    for matcher in values.iter_mut() {
        matcher.source =
            canonical_rooted_chain(&normalize_member_chain(&matcher.source)).to_string();
        matcher.member = matcher.member.trim().to_string();
    }
    values.retain(|matcher| !matcher.source.is_empty() && !matcher.member.is_empty());
    values.sort_by(|left, right| (&left.source, &left.member).cmp(&(&right.source, &right.member)));
    values.dedup();
}

pub(super) fn normalize_returned_member_reads(values: &mut Vec<ReturnedMemberReadMatcher>) {
    for matcher in values.iter_mut() {
        matcher.source =
            canonical_rooted_chain(&normalize_member_chain(&matcher.source)).to_string();
        matcher.member = matcher.member.trim().to_string();
    }
    values.retain(|matcher| !matcher.source.is_empty() && !matcher.member.is_empty());
    values.sort_by(|left, right| (&left.source, &left.member).cmp(&(&right.source, &right.member)));
    values.dedup();
}

pub(super) fn normalize_instance_member_calls(values: &mut Vec<InstanceMemberCallMatcher>) {
    for matcher in values.iter_mut() {
        matcher.module = matcher.module.trim().to_string();
        matcher.export = matcher.export.trim().to_string();
        matcher.member = matcher.member.trim().to_string();
    }
    values.retain(|matcher| {
        !matcher.module.is_empty() && !matcher.export.is_empty() && !matcher.member.is_empty()
    });
    values.sort_by(|left, right| {
        (&left.module, &left.export, &left.member).cmp(&(
            &right.module,
            &right.export,
            &right.member,
        ))
    });
    values.dedup();
}

pub(super) fn normalize_flows(values: &mut Vec<FlowMatcher>) {
    for flow in values.iter_mut() {
        flow.symbol = flow.symbol.trim().to_string();
        for source in &mut flow.sources {
            source.member_call = normalize_member_chain(&source.member_call);
            for matcher in &mut source.arg_strings {
                normalize_strings(&mut matcher.values);
                if let Some(predicate) = &mut matcher.predicate {
                    normalize_flow_value(predicate);
                }
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

pub(super) fn normalize_flow_value(value: &mut FlowValueMatcher) {
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

pub(super) fn normalize_call_provenance(provenance: &mut CallProvenance) {
    if let CallProvenance::ModuleExport { module } = provenance {
        *module = module.trim().to_string();
    }
}

pub(super) fn normalize_class_matchers(values: &mut Vec<ClassMatcher>) {
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

pub(super) fn normalize_constructor_matchers(values: &mut Vec<ConstructorMatcher>) {
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

pub(super) fn normalize_strings(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    for value in values.iter_mut() {
        *value = value.trim().to_string();
    }
    values.sort();
    values.dedup();
}

pub(super) fn normalize_member_chains(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    for value in values.iter_mut() {
        *value = normalize_member_chain(value);
        *value = canonical_rooted_chain(value).to_string();
    }
    values.retain(|value| !value.is_empty());
    values.sort();
    values.dedup();
}

pub(super) fn normalize_member_chain(value: &str) -> String {
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_is_idempotent_and_deduplicates_argument_matchers() {
        let matcher = ApiMatcher::from_matchers(vec![
            Matcher::member_call(
                MemberCallMatcher::rooted_chain(" this.client.request ")
                    .arg_string(1, [" b ", "a"])
                    .arg_string(0, ["value"]),
            ),
            Matcher::member_call(
                MemberCallMatcher::rooted_chain("this.client.request")
                    .arg_string(0, ["value"])
                    .arg_string(1, ["a", "b"]),
            ),
        ]);
        let once = matcher.normalized();
        assert_eq!(once.member_calls, once.clone().normalized().member_calls);
        assert_eq!(once.member_calls.len(), 1);
        assert_eq!(once.member_calls[0].arg_strings.len(), 2);
        assert_eq!(once.member_calls[0].arg_strings[0].values, ["value"]);
        assert_eq!(once.member_calls[0].arg_strings[1].values, ["a", "b"]);
    }

    #[test]
    fn normalization_is_permutation_invariant() {
        let values = vec![
            Matcher::global_call(" fetch "),
            Matcher::rooted_member_read("this.client.request"),
            Matcher::import("sdk"),
            Matcher::string_literal("marker"),
        ];
        let forward = ApiMatcher::from_matchers(values.clone()).normalized();
        let reverse = ApiMatcher::from_matchers(values.into_iter().rev().collect()).normalized();
        assert_eq!(forward.into_matchers(), reverse.into_matchers());
    }
}
