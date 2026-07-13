mod call;
mod derived;
mod flow;
mod member;

pub use call::*;
pub use derived::*;
pub use flow::*;
pub use member::*;

#[derive(Debug, Clone, Default)]
pub(crate) struct ApiMatcher {
    pub(crate) calls: Vec<CallMatcher>,
    pub(crate) member_calls: Vec<MemberCallMatcher>,
    pub(crate) member_reads: Vec<MemberReadMatcher>,
    pub(crate) imports: Vec<String>,
    pub(crate) string_literals: Vec<String>,
    pub(crate) classes: Vec<ClassMatcher>,
    pub(crate) constructors: Vec<ConstructorMatcher>,
    pub(crate) flows: Vec<ObjectFlowMatcher>,
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
    ObjectFlow(ObjectFlowMatcher),
    ReturnedMemberCall(ReturnedMemberCallMatcher),
    ReturnedMemberRead(ReturnedMemberReadMatcher),
    InstanceMemberCall(InstanceMemberCallMatcher),
}

impl Matcher {
    pub fn call(value: CallMatcher) -> Self {
        value.into()
    }
    pub fn global_call(name: impl Into<String>) -> Self {
        CallMatcher::global(name).into()
    }
    pub fn heuristic_call(name: impl Into<String>) -> Self {
        CallMatcher::heuristic(name).into()
    }
    pub fn module_call(module: impl Into<String>, export: impl Into<String>) -> Self {
        CallMatcher::module_export(module, export).into()
    }
    pub fn member_call(value: MemberCallMatcher) -> Self {
        value.into()
    }
    pub fn heuristic_member_call(chain: impl Into<String>) -> Self {
        MemberCallMatcher::heuristic(chain).into()
    }
    pub fn rooted_member_call(chain: impl Into<String>) -> Self {
        MemberCallMatcher::rooted(chain).into()
    }
    pub fn module_member_call(module: impl Into<String>, member: impl Into<String>) -> Self {
        MemberCallMatcher::module_member(module, member).into()
    }
    pub fn member_read(value: MemberReadMatcher) -> Self {
        value.into()
    }
    pub fn heuristic_member_read(chain: impl Into<String>) -> Self {
        MemberReadMatcher::heuristic(chain).into()
    }
    pub fn rooted_member_read(chain: impl Into<String>) -> Self {
        MemberReadMatcher::rooted(chain).into()
    }
    pub fn module_member_read(module: impl Into<String>, member: impl Into<String>) -> Self {
        MemberReadMatcher::module_member(module, member).into()
    }
    pub fn import(module: impl Into<String>) -> Self {
        Self::Import(module.into())
    }
    pub fn string_literal(value: impl Into<String>) -> Self {
        Self::StringLiteral(value.into())
    }
    pub fn class(value: ClassMatcher) -> Self {
        value.into()
    }
    pub fn heuristic_class(name: impl Into<String>) -> Self {
        ClassMatcher::heuristic(name).into()
    }
    pub fn module_class(module: impl Into<String>, export: impl Into<String>) -> Self {
        ClassMatcher::module_export(module, export).into()
    }
    pub fn constructor(value: ConstructorMatcher) -> Self {
        value.into()
    }
    pub fn heuristic_constructor(name: impl Into<String>) -> Self {
        ConstructorMatcher::heuristic(name).into()
    }
    pub fn global_constructor(name: impl Into<String>) -> Self {
        ConstructorMatcher::global(name).into()
    }
    pub fn module_constructor(module: impl Into<String>, export: impl Into<String>) -> Self {
        ConstructorMatcher::module_export(module, export).into()
    }
    pub fn flow(value: impl Into<Matcher>) -> Self {
        value.into()
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
impl From<ObjectFlowMatcher> for Matcher {
    fn from(value: ObjectFlowMatcher) -> Self {
        Self::ObjectFlow(value)
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

impl ApiMatcher {
    pub(crate) fn from_matchers(matchers: Vec<Matcher>) -> Self {
        let mut api_matcher = Self::default();
        for matcher in matchers {
            api_matcher.push(matcher);
        }
        api_matcher
    }

    pub(crate) fn into_matchers(self) -> Vec<Matcher> {
        self.calls
            .into_iter()
            .map(Matcher::Call)
            .chain(self.member_calls.into_iter().map(Matcher::MemberCall))
            .chain(self.member_reads.into_iter().map(Matcher::MemberRead))
            .chain(self.imports.into_iter().map(Matcher::Import))
            .chain(self.string_literals.into_iter().map(Matcher::StringLiteral))
            .chain(self.classes.into_iter().map(Matcher::Class))
            .chain(self.constructors.into_iter().map(Matcher::Constructor))
            .chain(self.flows.into_iter().map(Matcher::ObjectFlow))
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

    pub(crate) fn push(&mut self, matcher: Matcher) {
        match matcher {
            Matcher::Call(value) => self.calls.push(value),
            Matcher::MemberCall(value) => self.member_calls.push(value),
            Matcher::MemberRead(value) => self.member_reads.push(value),
            Matcher::Import(value) => self.imports.push(value),
            Matcher::StringLiteral(value) => self.string_literals.push(value),
            Matcher::Class(value) => self.classes.push(value),
            Matcher::Constructor(value) => self.constructors.push(value),
            Matcher::ObjectFlow(value) => self.flows.push(value),
            Matcher::ReturnedMemberCall(value) => self.returned_member_calls.push(value),
            Matcher::ReturnedMemberRead(value) => self.returned_member_reads.push(value),
            Matcher::InstanceMemberCall(value) => self.instance_member_calls.push(value),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        super::validation::validate(self)
    }

    pub(crate) fn is_empty(&self) -> bool {
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

    pub(crate) fn normalized(self) -> Self {
        super::normalization::normalize(self)
    }
}

pub(crate) fn normalize_returned_member_calls(values: &mut Vec<ReturnedMemberCallMatcher>) {
    for matcher in values.iter_mut() {
        matcher.source =
            canonical_rooted_chain(&normalize_member_chain(&matcher.source)).to_string();
        matcher.member = matcher.member.trim().to_string();
    }
    values.retain(|matcher| !matcher.source.is_empty() && !matcher.member.is_empty());
    values.sort_by(|left, right| (&left.source, &left.member).cmp(&(&right.source, &right.member)));
    values.dedup();
}

pub(crate) fn normalize_returned_member_reads(values: &mut Vec<ReturnedMemberReadMatcher>) {
    for matcher in values.iter_mut() {
        matcher.source =
            canonical_rooted_chain(&normalize_member_chain(&matcher.source)).to_string();
        matcher.member = matcher.member.trim().to_string();
    }
    values.retain(|matcher| !matcher.source.is_empty() && !matcher.member.is_empty());
    values.sort_by(|left, right| (&left.source, &left.member).cmp(&(&right.source, &right.member)));
    values.dedup();
}

pub(crate) fn normalize_instance_member_calls(values: &mut Vec<InstanceMemberCallMatcher>) {
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

pub(crate) fn normalize_flows(values: &mut Vec<ObjectFlowMatcher>) {
    for flow in values.iter_mut() {
        flow.symbol = flow.symbol.trim().to_string();
        for source in &mut flow.sources {
            normalize_member_call(&mut source.call);
        }
        if let Some(condition) = &mut flow.condition {
            normalize_condition(condition);
        }
        if let Some(completion) = &mut flow.completion {
            normalize_completion(completion);
        }
    }
    values.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    values.dedup();
}

fn normalize_condition(condition: &mut FlowCondition) {
    let events = match condition {
        FlowCondition::AnyOf(events) | FlowCondition::AllOf(events) => events,
    };
    for event in events {
        normalize_event(event);
    }
}

fn normalize_event(event: &mut ObjectEventMatcher) {
    match event {
        ObjectEventMatcher::PropertyWrite { property, value } => {
            *property = property.trim().to_string();
            normalize_value(value);
        }
        ObjectEventMatcher::MemberCall { member, arguments } => {
            *member = member.trim().to_string();
            normalize_arguments(arguments);
        }
    }
}

fn normalize_completion(completion: &mut FlowCompletion) {
    if let FlowCompletion::AnySink(sinks) = completion {
        for sink in sinks {
            match sink {
                FlowSinkMatcher::ArgumentOf { call, .. }
                | FlowSinkMatcher::AnyArgumentOf { call } => normalize_member_call(call),
            }
        }
    }
}

fn normalize_member_call(call: &mut MemberCallMatcher) {
    call.chain = normalize_member_chain(&call.chain);
    if call.provenance == MemberCallProvenance::Rooted {
        call.chain = canonical_rooted_chain(&call.chain).to_string();
    }
    if let MemberCallProvenance::ModuleNamespace { module } = &mut call.provenance {
        *module = module.trim().to_string();
    }
    normalize_arguments(&mut call.arguments);
}

pub(crate) fn normalize_arguments(arguments: &mut Vec<ArgumentConstraint>) {
    for argument in &mut *arguments {
        normalize_argument(&mut argument.matcher);
    }
    arguments.sort_by_key(|argument| argument.index);
    arguments.dedup();
}

fn normalize_argument(argument: &mut ArgumentMatcher) {
    match argument {
        ArgumentMatcher::Value(value) => normalize_value(value),
        ArgumentMatcher::ObjectKeys(keys) | ArgumentMatcher::RootedExpressions(keys) => {
            normalize_strings(keys)
        }
    }
}

pub(crate) fn normalize_value(value: &mut ValueMatcher) {
    if let ValueMatcherKind::StaticString(predicate) = &mut value.kind {
        match predicate {
            StaticStringPredicate::Any => {}
            StaticStringPredicate::Exact(values)
            | StaticStringPredicate::Prefix(values)
            | StaticStringPredicate::ContainsAny(values)
            | StaticStringPredicate::ContainsAll(values) => normalize_strings(values),
        }
    }
}

pub(crate) fn normalize_strings(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    for value in values.iter_mut() {
        *value = value.trim().to_string();
    }
    values.sort();
    values.dedup();
}

pub(crate) fn normalize_member_chain(value: &str) -> String {
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
