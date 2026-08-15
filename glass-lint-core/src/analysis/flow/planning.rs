//! Pre-bound flow sources, requirements, and sinks.
//!
//! Constructed once per module between catalog compilation and flow
//! execution. Symbol paths are resolved to `NamePath` once so that
//! repeating `NamePath::from_symbol_path` calls during local and
//! cross-module projection are eliminated. Sources and sinks are indexed
//! by member-call chain for O(log n) lookup per chain instead of O(n)
//! per call.

use std::collections::BTreeMap;

use glass_lint_datastructures::{NamePath, NameTable};
use smol_str::SmolStr;

use crate::{
    analysis::{
        facts::CallArgInfo,
        flow::effect::CallShape,
        model::{
            flow::{FlowId, RequirementIndex, SinkIndex},
            value::ValueTable,
        },
    },
    api::{
        classification::RuleIndex,
        compiler::{
            CompiledObjectFlow,
            normalized::CanonicalArgumentConstraints,
            object_flow::{CompiledObjectSink, CompiledObjectSinkArguments},
        },
        rule::{ArgumentIndex, ArgumentMatcher, query::lifecycle::LifecycleCallTarget},
    },
};

#[derive(Debug)]
pub(super) struct FlowMatchView<'a> {
    names: &'a NameTable,
    values: &'a ValueTable,
}

impl<'a> FlowMatchView<'a> {
    pub(super) fn new(names: &'a NameTable, values: &'a ValueTable) -> Self {
        Self { names, values }
    }

    pub(super) fn argument_matches_predicate(
        &self,
        index: ArgumentIndex,
        matcher: &ArgumentMatcher,
        args: &[CallArgInfo],
    ) -> bool {
        args.get(index.get())
            .is_some_and(|argument| matcher.matches(argument, self.names, self.values))
    }

    pub(super) fn arguments_match(
        &self,
        matchers: &CanonicalArgumentConstraints,
        args: &[CallArgInfo],
    ) -> bool {
        matchers
            .iter()
            .all(|(index, matcher)| self.argument_matches_predicate(index, matcher, args))
    }

    pub(super) fn member_matches(actual: &NamePath, expected: &NamePath) -> bool {
        actual == expected || actual.last_segment() == expected.last_segment()
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum BoundLifecycleCallTarget {
    Member(NamePath),
    Global(SmolStr),
}

impl BoundLifecycleCallTarget {
    pub(super) fn from_lifecycle(target: &LifecycleCallTarget, names: &NameTable) -> Option<Self> {
        match target {
            LifecycleCallTarget::RootedMember(path) => names.lookup_path(path).map(Self::Member),
            LifecycleCallTarget::Global(name) => Some(Self::Global(name.clone())),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct BoundTargetIndex<T> {
    globals: BTreeMap<SmolStr, Vec<T>>,
    members: BTreeMap<NamePath, Vec<T>>,
}

impl<T> Default for BoundTargetIndex<T> {
    fn default() -> Self {
        Self {
            globals: BTreeMap::new(),
            members: BTreeMap::new(),
        }
    }
}

impl<T> BoundTargetIndex<T> {
    pub(super) fn insert(&mut self, target: BoundLifecycleCallTarget, value: T) {
        match target {
            BoundLifecycleCallTarget::Global(name) => {
                self.globals.entry(name).or_default().push(value);
            }
            BoundLifecycleCallTarget::Member(path) => {
                self.members.entry(path).or_default().push(value);
            }
        }
    }

    pub(super) fn candidates_for_call(&self, call: &CallShape<'_>) -> Option<&[T]> {
        call.global_name()
            .and_then(|name| self.globals.get(name).map(Vec::as_slice))
            .or_else(|| {
                call.rooted()
                    .then(|| call.chain())
                    .flatten()
                    .and_then(|chain| self.members.get(chain).map(Vec::as_slice))
            })
    }
}

impl<T: Ord> BoundTargetIndex<T> {
    pub(super) fn normalize(&mut self) {
        for values in self.globals.values_mut().chain(self.members.values_mut()) {
            values.sort_unstable();
            values.dedup();
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PropertyRequirementMatch {
    index: RequirementIndex,
    value_matches: bool,
}

impl PropertyRequirementMatch {
    pub(super) fn index(self) -> RequirementIndex {
        self.index
    }

    pub(super) fn value_matches(self) -> bool {
        self.value_matches
    }
}

/// Build a deterministic `BoundSource` index from compiled lifecycle
/// declarations. Target binding, `BoundSource` construction, and
/// normalization stay owned by this planning boundary.
pub(super) fn build_bound_source_index<'rules>(
    flows: impl IntoIterator<Item = (FlowId, &'rules CompiledObjectFlow)>,
    names: &NameTable,
) -> BoundTargetIndex<BoundSource> {
    let mut index = BoundTargetIndex::default();
    for (flow_id, flow) in flows {
        for source in flow.sources() {
            if let Some(target) = BoundLifecycleCallTarget::from_lifecycle(source.target(), names) {
                index.insert(
                    target,
                    BoundSource::new(flow_id, source.argument_constraints().clone()),
                );
            }
        }
    }
    index.normalize();
    index
}

/// One lifecycle root with the identity assigned by the physical-plan
/// boundary. Local and cross-module flow consumers share this exact entry.
#[derive(Debug, Clone, Copy)]
pub(in crate::analysis) struct BoundLifecycleRoot<'rules> {
    flow_id: FlowId,
    flow: &'rules CompiledObjectFlow,
}

impl<'rules> BoundLifecycleRoot<'rules> {
    pub(in crate::analysis) fn new(
        rule_index: RuleIndex,
        root_index: usize,
        flow: &'rules CompiledObjectFlow,
    ) -> Self {
        Self {
            flow_id: FlowId::new(rule_index, root_index),
            flow,
        }
    }

    pub(in crate::analysis) fn flow_id(self) -> FlowId {
        self.flow_id
    }

    pub(in crate::analysis) fn flow(self) -> &'rules CompiledObjectFlow {
        self.flow
    }
}

#[derive(Debug, Clone)]
pub(super) struct BoundFlowPlan<'rules> {
    flows: BTreeMap<FlowId, &'rules CompiledObjectFlow>,
    sources: BoundTargetIndex<BoundSource>,
    sinks: BoundTargetIndex<BoundSink>,
    /// Pre-resolved requirement member paths per flow, indexed by
    /// requirement position.  `None` for PropertyWrite requirements
    /// (which have no member-call path).
    req_members: BTreeMap<FlowId, Vec<Option<NamePath>>>,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct BoundSource {
    flow: FlowId,
    arguments: CanonicalArgumentConstraints,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct BoundSink {
    flow: FlowId,
    index: SinkIndex,
    arguments: CompiledObjectSinkArguments,
}

impl BoundSink {
    fn new(flow: FlowId, index: SinkIndex, sink: &CompiledObjectSink) -> Self {
        Self {
            flow,
            index,
            arguments: sink.arguments().clone(),
        }
    }

    pub(super) fn flow_id(&self) -> FlowId {
        self.flow
    }

    pub(super) fn index(&self) -> SinkIndex {
        self.index
    }

    pub(super) fn matches_argument(&self, argument: usize) -> bool {
        self.arguments.matches_argument(argument)
    }

    pub(super) fn present_indices(
        &self,
        argument_count: usize,
    ) -> impl Iterator<Item = usize> + '_ {
        self.arguments.present_indices(argument_count)
    }
}

impl BoundSource {
    pub(super) fn new(flow: FlowId, arguments: CanonicalArgumentConstraints) -> Self {
        Self { flow, arguments }
    }

    pub(super) fn flow_id(&self) -> FlowId {
        self.flow
    }

    pub(super) fn matches_call(&self, matcher: &FlowMatchView<'_>, args: &[CallArgInfo]) -> bool {
        matcher.arguments_match(&self.arguments, args)
    }
}

impl<'rules> BoundFlowPlan<'rules> {
    /// Build a plan from compiled flow matchers.
    pub(super) fn new(roots: &[BoundLifecycleRoot<'rules>], names: &NameTable) -> Self {
        let mut flows = BTreeMap::new();
        let mut sinks = BoundTargetIndex::default();
        let mut req_members = BTreeMap::new();

        for root in roots {
            let id = root.flow_id();
            let flow = root.flow();
            flows.insert(id, flow);

            for (sink_index, sink) in flow.sinks().enumerate() {
                if let Some(target) = BoundLifecycleCallTarget::from_lifecycle(sink.target(), names)
                {
                    let index = SinkIndex::new(sink_index)
                        .expect("validated sink index is within 64 entries");
                    sinks.insert(target, BoundSink::new(id, index, sink));
                }
            }

            req_members.insert(id, Self::build_requirement_members(flow, names));
        }

        let sources = build_bound_source_index(
            roots.iter().map(|root| (root.flow_id(), root.flow())),
            names,
        );
        sinks.normalize();

        Self {
            flows,
            sources,
            sinks,
            req_members,
        }
    }

    fn build_requirement_members(
        flow: &CompiledObjectFlow,
        names: &NameTable,
    ) -> Vec<Option<NamePath>> {
        flow.requirements()
            .map(|requirement| {
                requirement
                    .member_call()
                    .and_then(|(member, _)| names.lookup_path(member))
            })
            .collect()
    }

    /// Look up a compiled flow by its stable identifier.
    pub(super) fn get(&self, id: FlowId) -> Option<&CompiledObjectFlow> {
        self.flows.get(&id).copied()
    }

    /// Select executable source candidates using the canonical call-target
    /// precedence: a global target first, then a rooted member target.
    pub(super) fn source_candidates_for_call(
        &self,
        call: &CallShape<'_>,
    ) -> Option<&[BoundSource]> {
        self.sources.candidates_for_call(call)
    }

    /// Select sink candidates using the same global-before-rooted policy as
    /// source selection.
    pub(super) fn sink_candidates_for_call(&self, call: &CallShape<'_>) -> Option<&[BoundSink]> {
        self.sinks.candidates_for_call(call)
    }

    pub(super) fn matching_member_requirement_indices(
        &self,
        flow_id: FlowId,
        actual: Option<&NamePath>,
        args: &[CallArgInfo],
        matcher: &FlowMatchView<'_>,
    ) -> Vec<RequirementIndex> {
        let Some(actual) = actual else {
            return Vec::new();
        };
        let Some(flow) = self.get(flow_id) else {
            return Vec::new();
        };
        let Some(members) = self.req_members.get(&flow_id) else {
            return Vec::new();
        };
        members
            .iter()
            .zip(flow.requirements())
            .enumerate()
            .filter_map(|(index, (member, requirement))| {
                let member = member.as_ref()?;
                (FlowMatchView::member_matches(actual, member)
                    && requirement
                        .member_call()
                        .is_some_and(|(_, arguments)| matcher.arguments_match(arguments, args)))
                .then(|| RequirementIndex::new(index))
                .flatten()
            })
            .collect()
    }

    pub(super) fn matching_property_requirements(
        &self,
        flow_id: FlowId,
        property: Option<&str>,
        static_value: Option<&str>,
        value_is_precise: bool,
    ) -> Vec<PropertyRequirementMatch> {
        self.get(flow_id)
            .into_iter()
            .flat_map(CompiledObjectFlow::requirements)
            .enumerate()
            .filter_map(|(index, requirement)| {
                let (expected, matcher) = requirement.property_write()?;
                (property.is_none() || property == Some(expected.as_str())).then_some(
                    PropertyRequirementMatch {
                        index: RequirementIndex::new(index)?,
                        value_matches: value_is_precise
                            && property == Some(expected.as_str())
                            && matcher.matches_flow_value(static_value),
                    },
                )
            })
            .collect()
    }
}
