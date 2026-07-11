//! Bounded, intra-function object flow for declarative flow matchers.
//!
//! A flow state begins at a configured source call, follows direct aliases,
//! accumulates configuration requirements, and is emitted only at a matching
//! sink. The analysis deliberately does not merge control-flow branches: that
//! conservative choice avoids turning an uncertain value into a report. Flow
//! evidence is anchored at the source allocation. This is the flow match site:
//! it gives one stable finding for one allocated object even if that object is
//! passed to the same sink repeatedly. Requirement-only flows use the same
//! source anchor.

use std::collections::{BTreeMap, BTreeSet};

use swc_common::{Span, Spanned};
use swc_ecma_ast::{
    AssignExpr, AssignTarget, BlockStmt, CallExpr, Callee, CondExpr, DoWhileStmt, Expr,
    ExprOrSpread, FnDecl, ForInStmt, ForOfStmt, ForStmt, Function, IfStmt, MemberExpr,
    OptChainBase, OptChainExpr, Pat, Program, SimpleAssignTarget, SwitchStmt, TryStmt,
    VarDeclarator, WhileStmt,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::super::result::{ApiEvidence, ApiMatchKind};
use super::super::rule::{
    ArgStringMatcher, FlowMatcher, FlowRequirement, FlowSinkArgs, FlowValueMatcher,
};
use super::ast::{binding_ident_name, member_chain, member_prop_name};
use super::resolver::Resolver;
use super::value::ObjectId;

const MAX_FLOW_OBJECTS: u32 = 65_536;
const MAX_FLOW_STATES: usize = 262_144;
const MAX_FLOW_EMISSIONS: usize = 65_536;

#[derive(Debug, Clone, Copy)]
pub(super) struct FlowLimits {
    pub(super) max_objects: u32,
    pub(super) max_states: usize,
    pub(super) max_emissions: usize,
}

impl Default for FlowLimits {
    fn default() -> Self {
        Self {
            max_objects: MAX_FLOW_OBJECTS,
            max_states: MAX_FLOW_STATES,
            max_emissions: MAX_FLOW_EMISSIONS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct FlowId {
    rule_index: usize,
    flow_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct BindingKey {
    function_id: usize,
    path: String,
}

#[derive(Debug, Default)]
struct FlowIndex {
    flows: BTreeMap<FlowId, FlowMatcher>,
    sources: BTreeMap<String, Vec<FlowId>>,
    sinks: BTreeMap<String, Vec<FlowId>>,
}

impl FlowIndex {
    fn new(rules: &[(usize, usize, FlowMatcher)]) -> Self {
        let mut index = Self::default();
        for (rule_index, flow_index, flow) in rules {
            let id = FlowId {
                rule_index: *rule_index,
                flow_index: *flow_index,
            };
            index.flows.insert(id, flow.clone());
            for source in &flow.sources {
                index
                    .sources
                    .entry(source.member_call.clone())
                    .or_default()
                    .push(id);
            }
            for sink in &flow.sinks {
                for member_call in &sink.member_calls {
                    index.sinks.entry(member_call.clone()).or_default().push(id);
                }
            }
        }
        for ids in index.sources.values_mut().chain(index.sinks.values_mut()) {
            ids.sort_unstable();
            ids.dedup();
        }
        index
    }

    fn get(&self, id: FlowId) -> Option<&FlowMatcher> {
        self.flows.get(&id)
    }
}

pub fn collect(
    program: &Program,
    resolver: &Resolver,
    rules: &[(usize, usize, FlowMatcher)],
    rule_count: usize,
) -> Vec<Vec<ApiEvidence>> {
    collect_with_limits(program, resolver, rules, rule_count, FlowLimits::default())
}

pub(super) fn collect_with_limits(
    program: &Program,
    resolver: &Resolver,
    rules: &[(usize, usize, FlowMatcher)],
    rule_count: usize,
    limits: FlowLimits,
) -> Vec<Vec<ApiEvidence>> {
    let flow_index = FlowIndex::new(rules);
    let helpers = HelperCollector::collect(program, resolver, &flow_index);
    let mut visitor = ObjectFlowCollector {
        resolver,
        flow_index,
        helpers,
        evidence: vec![Vec::new(); rule_count],
        aliases: BTreeMap::new(),
        states: BTreeMap::new(),
        emitted: BTreeSet::new(),
        function_stack: vec![0],
        next_function_id: 1,
        next_object_id: 0,
        limits,
    };
    program.visit_with(&mut visitor);
    visitor.evidence
}

#[derive(Debug, Clone)]
struct FlowState {
    flow: FlowId,
    source_span: Span,
    object_id: ObjectId,
    requirements: BTreeSet<usize>,
    emitted: bool,
}

#[derive(Debug, Clone)]
struct HelperSink {
    flow: FlowId,
    param_index: usize,
    parameter_count: usize,
}

struct ObjectFlowCollector<'a> {
    resolver: &'a Resolver,
    flow_index: FlowIndex,
    helpers: BTreeMap<(usize, String), Vec<HelperSink>>,
    evidence: Vec<Vec<ApiEvidence>>,
    aliases: BTreeMap<BindingKey, ObjectId>,
    states: BTreeMap<(ObjectId, FlowId), FlowState>,
    emitted: BTreeSet<(usize, usize, ObjectId, u32, u32)>,
    function_stack: Vec<usize>,
    next_function_id: usize,
    next_object_id: u32,
    limits: FlowLimits,
}

impl ObjectFlowCollector<'_> {
    fn current_function(&self) -> usize {
        self.function_stack.last().copied().unwrap_or(0)
    }

    fn scoped_key(&self, chain: &str) -> BindingKey {
        BindingKey {
            function_id: self.current_function(),
            path: chain.to_string(),
        }
    }

    fn expr_key(&self, expr: &Expr) -> Option<BindingKey> {
        self.resolver
            .rooted_expr_chain(expr)
            .map(|chain| self.scoped_key(&chain))
    }

    fn member_object_key(&self, member: &MemberExpr) -> Option<BindingKey> {
        self.expr_key(&member.obj)
    }

    fn source_invocation<'a>(
        &self,
        expression: &'a Expr,
    ) -> Option<(String, &'a [ExprOrSpread], Span)> {
        match expression {
            Expr::Call(call) => {
                let span = if call.span.is_dummy() {
                    expression.span()
                } else {
                    call.span
                };
                Some((call_member_chain(call, self.resolver)?, &call.args, span))
            }
            Expr::OptChain(chain) => match &*chain.base {
                OptChainBase::Call(call) => {
                    let span = if call.span.is_dummy() {
                        expression.span()
                    } else {
                        call.span
                    };
                    Some((
                        member_callee_chain(&call.callee, self.resolver)?,
                        &call.args,
                        span,
                    ))
                }
                OptChainBase::Member(_) => None,
            },
            Expr::Paren(paren) => self.source_invocation(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expression| self.source_invocation(expression)),
            Expr::TsAs(value) => self.source_invocation(&value.expr),
            Expr::TsNonNull(value) => self.source_invocation(&value.expr),
            Expr::TsSatisfies(value) => self.source_invocation(&value.expr),
            Expr::TsTypeAssertion(value) => self.source_invocation(&value.expr),
            _ => None,
        }
    }

    fn source_match(
        &mut self,
        callee: &str,
        args: &[ExprOrSpread],
        span: Span,
    ) -> Option<(ObjectId, Vec<FlowState>)> {
        let matching = self
            .flow_index
            .sources
            .get(callee)
            .into_iter()
            .flatten()
            .copied()
            .filter(|flow_id| {
                self.flow_index.get(*flow_id).is_some_and(|flow| {
                    flow.sources.iter().any(|source| {
                        source.member_call == callee
                            && source
                                .arg_strings
                                .iter()
                                .all(|matcher| static_arg_matches(matcher, args, self.resolver))
                    })
                })
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            return None;
        }
        let object_id = self.allocate_object_id()?;
        let states = matching
            .into_iter()
            .map(|flow| FlowState {
                flow,
                source_span: span,
                object_id,
                requirements: BTreeSet::new(),
                emitted: false,
            })
            .collect();
        Some((object_id, states))
    }

    fn assign_source_or_clear(&mut self, target: &Expr, value: &Expr) {
        let Some(key) = self.expr_key(target) else {
            return;
        };
        if let Some((callee, args, span)) = self.source_invocation(value)
            && let Some((object_id, states)) = self.source_match(&callee, args, span)
        {
            self.bind_states(key, object_id, states);
            return;
        }
        // Only copy a state through a direct identifier alias. Following
        // arbitrary expressions here would require control-flow and mutation
        // reasoning that this intentionally small analysis does not provide.
        if matches!(value, Expr::Ident(_))
            && let Some(source_key) = self.expr_key(value)
            && let Some(object_id) = self.aliases.get(&source_key).copied()
        {
            self.aliases.insert(key, object_id);
            return;
        }
        self.aliases.remove(&key);
    }

    fn bind_states(&mut self, key: BindingKey, object_id: ObjectId, states: Vec<FlowState>) {
        self.aliases.insert(key, object_id);
        if self.states.len().saturating_add(states.len()) > self.limits.max_states {
            return;
        }
        for state in states {
            self.states.insert((object_id, state.flow), state);
        }
    }

    fn allocate_object_id(&mut self) -> Option<ObjectId> {
        if self.next_object_id >= self.limits.max_objects || self.next_object_id >= MAX_FLOW_OBJECTS
        {
            return None;
        }
        let object = ObjectId(self.next_object_id);
        self.next_object_id = self.next_object_id.checked_add(1)?;
        Some(object)
    }

    fn object_for_key(&self, key: &BindingKey) -> Option<ObjectId> {
        self.aliases.get(key).copied()
    }

    fn record_property_write(&mut self, member: &MemberExpr, value: &Expr) {
        let Some(key) = self.member_object_key(member) else {
            return;
        };
        let Some(property) = member_prop_name(&member.prop) else {
            return;
        };
        let static_value = self.resolver.static_string_expr(value);
        self.update_requirements(&key, |_flow, requirement| match requirement {
            FlowRequirement::PropertyWrite {
                property: expected,
                value,
            } => expected == &property && flow_value_matches(value, static_value.as_deref(), true),
            FlowRequirement::MemberCall { .. } => false,
        });
    }

    fn record_member_configuration(&mut self, member: &MemberExpr, call: &CallExpr) {
        let Some(key) = self.member_object_key(member) else {
            return;
        };
        let Some(called_member) = member_prop_name(&member.prop) else {
            return;
        };
        self.update_requirements(&key, |_flow, requirement| match requirement {
            FlowRequirement::MemberCall { member, args } => {
                member == &called_member
                    && args.iter().all(|matcher| {
                        call.args.get(matcher.index).is_some_and(|arg| {
                            let value = self.resolver.static_string_expr(&arg.expr);
                            flow_value_matches(&matcher.value, value.as_deref(), true)
                        })
                    })
            }
            FlowRequirement::PropertyWrite { .. } => false,
        });
    }

    fn update_requirements(
        &mut self,
        key: &BindingKey,
        mut matches_requirement: impl FnMut(&FlowMatcher, &FlowRequirement) -> bool,
    ) {
        let Some(object_id) = self.object_for_key(key) else {
            return;
        };
        let state_keys = self
            .states
            .keys()
            .filter(|(object, _)| *object == object_id)
            .copied()
            .collect::<Vec<_>>();
        let mut ready = Vec::new();
        for state_key in state_keys {
            let Some(flow) = self.flow_index.get(state_key.1).cloned() else {
                continue;
            };
            let Some(state) = self.states.get_mut(&state_key) else {
                continue;
            };
            for (requirement_index, requirement) in flow.requirements.iter().enumerate() {
                if matches_requirement(&flow, requirement) {
                    state.requirements.insert(requirement_index);
                }
            }
            if flow.emit_on_requirements && state_is_ready(state, &flow) && !state.emitted {
                state.emitted = true;
                ready.push((state.clone(), flow.clone()));
            }
        }
        for (state, flow) in ready {
            self.emit_state_if_ready(&state, &flow);
        }
    }

    fn record_member_sink(&mut self, member: &MemberExpr, call: &CallExpr) {
        let Some(callee) = call_member_chain(call, self.resolver) else {
            return;
        };
        for (argument_index, argument) in call.args.iter().enumerate() {
            let Some(key) = self.expr_key(&argument.expr) else {
                continue;
            };
            self.emit_sink_matches(&key, &callee, argument_index);
        }
        if let Some(raw_callee) = member_chain(member)
            && raw_callee != callee
        {
            for (argument_index, argument) in call.args.iter().enumerate() {
                let Some(key) = self.expr_key(&argument.expr) else {
                    continue;
                };
                self.emit_sink_matches(&key, &raw_callee, argument_index);
            }
        }
    }

    fn record_helper_sink(&mut self, callee: &swc_ecma_ast::Ident, call: &CallExpr) {
        let Some((sinks, stable)) = self
            .resolver
            .scope_chain_at(callee.span)
            .into_iter()
            .find_map(|scope| {
                self.helpers
                    .get(&(scope, callee.sym.to_string()))
                    .cloned()
                    .map(|sinks| {
                        (
                            sinks,
                            !self
                                .resolver
                                .has_assignment_at(callee.sym.as_ref(), callee.span),
                        )
                    })
            })
        else {
            return;
        };
        if !stable {
            return;
        }
        for sink in sinks {
            // A helper summary is only valid for the invocation shape it was
            // derived from. Missing or extra arguments would otherwise make a
            // parameter look definitely aliased when JavaScript supplies
            // `undefined` or ignores an extra value.
            if call.args.len() != sink.parameter_count {
                continue;
            }
            let Some(argument) = call.args.get(sink.param_index) else {
                continue;
            };
            let Some(key) = self.expr_key(&argument.expr) else {
                continue;
            };
            self.emit_flow_if_ready(&key, sink.flow);
        }
    }

    fn record_identifier_sink(&mut self, callee: &str, call: &CallExpr) {
        for (argument_index, argument) in call.args.iter().enumerate() {
            let Some(key) = self.expr_key(&argument.expr) else {
                continue;
            };
            self.emit_sink_matches(&key, callee, argument_index);
        }
    }

    fn record_sink_arguments(&mut self, callee: &str, args: &[ExprOrSpread]) {
        for (argument_index, argument) in args.iter().enumerate() {
            let Some(key) = self.expr_key(&argument.expr) else {
                continue;
            };
            self.emit_sink_matches(&key, callee, argument_index);
        }
    }

    fn emit_sink_matches(&mut self, key: &BindingKey, callee: &str, argument_index: usize) {
        let Some(object_id) = self.object_for_key(key) else {
            return;
        };
        let candidate_flows = self
            .flow_index
            .sinks
            .get(callee)
            .cloned()
            .unwrap_or_default();
        let states = self
            .states
            .iter()
            .filter(|((object, flow), _)| *object == object_id && candidate_flows.contains(flow))
            .map(|(_, state)| state.clone())
            .collect::<Vec<_>>();
        for state in states {
            let Some(flow) = self.flow_index.get(state.flow).cloned() else {
                continue;
            };
            let sink_matches = flow.sinks.iter().any(|sink| {
                sink.member_calls
                    .iter()
                    .any(|member_call| member_call == callee)
                    && match &sink.args {
                        FlowSinkArgs::Any => true,
                        FlowSinkArgs::Indices(indices) => indices.contains(&argument_index),
                    }
            });
            if sink_matches {
                self.emit_state_if_ready(&state, &flow);
            }
        }
    }

    fn emit_flow_if_ready(&mut self, key: &BindingKey, flow: FlowId) {
        let Some(object_id) = self.object_for_key(key) else {
            return;
        };
        let Some(state) = self.states.get(&(object_id, flow)).cloned() else {
            return;
        };
        let Some(flow_matcher) = self.flow_index.get(flow).cloned() else {
            return;
        };
        self.emit_state_if_ready(&state, &flow_matcher);
    }

    fn emit_state_if_ready(&mut self, state: &FlowState, flow: &FlowMatcher) {
        if !state_is_ready(state, flow) {
            return;
        }
        let key = (
            state.flow.rule_index,
            state.flow.flow_index,
            state.object_id,
            state.source_span.lo.0,
            state.source_span.hi.0,
        );
        if !self.emitted.contains(&key) && self.emitted.len() >= self.limits.max_emissions {
            return;
        }
        if self.emitted.insert(key) {
            self.emit_state(state, flow, state.source_span);
        }
    }

    fn emit_state(&mut self, state: &FlowState, flow: &FlowMatcher, span: Span) {
        self.evidence[state.flow.rule_index].push(ApiEvidence {
            kind: ApiMatchKind::CallArgument,
            symbol: flow.evidence_symbol(),
            count: 1,
            spans: vec![span],
        });
    }

    fn enter_function(&mut self) {
        let id = self.next_function_id;
        let Some(next) = id.checked_add(1) else {
            return;
        };
        self.next_function_id = next;
        self.function_stack.push(id);
    }

    fn exit_function(&mut self) {
        debug_assert!(self.function_stack.len() > 1);
        let _ = self.function_stack.pop();
    }
}

fn state_is_ready(state: &FlowState, flow: &FlowMatcher) -> bool {
    if flow.all_requirements_required {
        state.requirements.len() == flow.requirements.len()
    } else {
        !state.requirements.is_empty()
    }
}

impl Visit for ObjectFlowCollector<'_> {
    fn visit_if_stmt(&mut self, statement: &IfStmt) {
        statement.test.visit_with(self);
        let baseline = self.states.clone();
        let baseline_aliases = self.aliases.clone();
        statement.cons.visit_with(self);
        self.states = baseline.clone();
        self.aliases = baseline_aliases.clone();
        if let Some(alternate) = &statement.alt {
            alternate.visit_with(self);
        }
        // A fact established in only one branch is not definite after the
        // join.  The branch-local visitors have already emitted valid
        // source-to-sink matches inside their own branch.
        self.states.clear();
        self.aliases.clear();
    }

    fn visit_cond_expr(&mut self, expression: &CondExpr) {
        expression.test.visit_with(self);
        let baseline = self.states.clone();
        let baseline_aliases = self.aliases.clone();
        expression.cons.visit_with(self);
        self.states = baseline.clone();
        self.aliases = baseline_aliases;
        expression.alt.visit_with(self);
        self.states.clear();
        self.aliases.clear();
    }

    fn visit_for_stmt(&mut self, statement: &ForStmt) {
        statement.init.visit_with(self);
        statement.test.visit_with(self);
        statement.update.visit_with(self);
        statement.body.visit_with(self);
        self.states.clear();
        self.aliases.clear();
    }

    fn visit_for_in_stmt(&mut self, statement: &ForInStmt) {
        statement.left.visit_with(self);
        statement.right.visit_with(self);
        statement.body.visit_with(self);
        self.states.clear();
        self.aliases.clear();
    }

    fn visit_for_of_stmt(&mut self, statement: &ForOfStmt) {
        statement.left.visit_with(self);
        statement.right.visit_with(self);
        statement.body.visit_with(self);
        self.states.clear();
        self.aliases.clear();
    }

    fn visit_while_stmt(&mut self, statement: &WhileStmt) {
        statement.test.visit_with(self);
        statement.body.visit_with(self);
        self.states.clear();
        self.aliases.clear();
    }

    fn visit_do_while_stmt(&mut self, statement: &DoWhileStmt) {
        statement.body.visit_with(self);
        statement.test.visit_with(self);
        self.states.clear();
        self.aliases.clear();
    }

    fn visit_switch_stmt(&mut self, statement: &SwitchStmt) {
        statement.discriminant.visit_with(self);
        let baseline = self.states.clone();
        let baseline_aliases = self.aliases.clone();
        for case in &statement.cases {
            self.states = baseline.clone();
            self.aliases = baseline_aliases.clone();
            case.visit_with(self);
        }
        self.states.clear();
        self.aliases.clear();
    }

    fn visit_try_stmt(&mut self, statement: &TryStmt) {
        let baseline = self.states.clone();
        let baseline_aliases = self.aliases.clone();
        statement.block.visit_with(self);
        self.states = baseline.clone();
        self.aliases = baseline_aliases;
        statement.handler.visit_with(self);
        if let Some(finalizer) = &statement.finalizer {
            finalizer.visit_with(self);
        }
        self.states.clear();
        self.aliases.clear();
    }

    fn visit_var_declarator(&mut self, declarator: &VarDeclarator) {
        if let (Pat::Ident(ident), Some(init)) = (&declarator.name, declarator.init.as_deref()) {
            self.assign_source_or_clear(&Expr::Ident(ident.id.clone()), init);
        }
        declarator.visit_children_with(self);
    }

    fn visit_assign_expr(&mut self, assignment: &AssignExpr) {
        match &assignment.left {
            AssignTarget::Simple(SimpleAssignTarget::Ident(ident)) => {
                self.assign_source_or_clear(&Expr::Ident(ident.id.clone()), &assignment.right);
            }
            AssignTarget::Simple(SimpleAssignTarget::Member(member)) => {
                self.record_property_write(member, &assignment.right);
                self.assign_source_or_clear(&Expr::Member(member.clone()), &assignment.right);
            }
            _ => {}
        }
        assignment.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        match &call.callee {
            Callee::Expr(callee) => match &**callee {
                Expr::Member(member) => {
                    self.record_member_configuration(member, call);
                    self.record_member_sink(member, call);
                }
                Expr::Ident(ident) => {
                    self.record_helper_sink(ident, call);
                    if let Some(callee) = self.resolver.resolve_ident(ident).rooted_chain {
                        self.record_identifier_sink(&callee, call);
                    }
                }
                _ => {}
            },
            Callee::Super(_) | Callee::Import(_) => {}
        }
        call.visit_children_with(self);
    }

    fn visit_opt_chain_expr(&mut self, chain: &OptChainExpr) {
        if let OptChainBase::Call(call) = &*chain.base
            && let Some(callee) = self.resolver.rooted_expr_chain(&call.callee)
        {
            self.record_sink_arguments(&callee, &call.args);
        }
        chain.visit_children_with(self);
    }

    fn visit_function(&mut self, function: &Function) {
        self.enter_function();
        function.visit_children_with(self);
        self.exit_function();
    }
}

struct HelperCollector<'a> {
    resolver: &'a Resolver,
    flow_index: &'a FlowIndex,
    helpers: BTreeMap<(usize, String), Vec<HelperSink>>,
}

impl<'a> HelperCollector<'a> {
    fn collect(
        program: &Program,
        resolver: &'a Resolver,
        flow_index: &'a FlowIndex,
    ) -> BTreeMap<(usize, String), Vec<HelperSink>> {
        let mut collector = Self {
            resolver,
            flow_index,
            helpers: BTreeMap::new(),
        };
        program.visit_with(&mut collector);
        collector.helpers
    }

    fn record_function(
        &mut self,
        scope: usize,
        name: String,
        parameters: Vec<String>,
        parameter_count: usize,
        body: Option<&BlockStmt>,
    ) {
        let Some(body) = body else {
            return;
        };
        self.record_helper(scope, name, parameters, parameter_count, |visitor| {
            body.visit_with(visitor)
        });
    }

    fn record_helper(
        &mut self,
        scope: usize,
        name: String,
        parameters: Vec<String>,
        parameter_count: usize,
        visit_body: impl FnOnce(&mut HelperBodyVisitor<'_>),
    ) {
        let mut visitor = HelperBodyVisitor {
            resolver: self.resolver,
            flow_index: self.flow_index,
            parameters,
            parameter_count,
            sinks: Vec::new(),
        };
        visit_body(&mut visitor);
        // Keep an empty marker as well: a nested function with the same name
        // must shadow an outer helper summary even when its body has no
        // modeled sink.
        self.helpers.insert((scope, name), visitor.sinks);
    }
}

impl Visit for HelperCollector<'_> {
    fn visit_fn_decl(&mut self, function: &FnDecl) {
        let parameters = function
            .function
            .params
            .iter()
            .filter_map(|param| binding_ident_name(&param.pat))
            .collect::<Vec<_>>();
        self.record_function(
            self.resolver
                .scope_chain_at(
                    function
                        .function
                        .body
                        .as_ref()
                        .map_or(function.ident.span, Spanned::span),
                )
                .get(2)
                .copied()
                .unwrap_or(0),
            function.ident.sym.to_string(),
            parameters,
            function.function.params.len(),
            function.function.body.as_ref(),
        );
        function.function.decorators.visit_with(self);
        if let Some(body) = &function.function.body {
            body.visit_with(self);
        }
    }

    fn visit_var_declarator(&mut self, declarator: &VarDeclarator) {
        let Pat::Ident(ident) = &declarator.name else {
            return;
        };
        let Some(init) = declarator.init.as_deref() else {
            return;
        };
        match init {
            Expr::Fn(function) => {
                let parameters = function
                    .function
                    .params
                    .iter()
                    .filter_map(|param| binding_ident_name(&param.pat))
                    .collect::<Vec<_>>();
                self.record_function(
                    self.resolver.scope_chain_at(ident.id.span)[0],
                    ident.id.sym.to_string(),
                    parameters,
                    function.function.params.len(),
                    function.function.body.as_ref(),
                );
                function.function.decorators.visit_with(self);
                if let Some(body) = &function.function.body {
                    body.visit_with(self);
                }
            }
            Expr::Arrow(arrow) => {
                let parameters = arrow
                    .params
                    .iter()
                    .filter_map(binding_ident_name)
                    .collect::<Vec<_>>();
                self.record_helper(
                    self.resolver.scope_chain_at(ident.id.span)[0],
                    ident.id.sym.to_string(),
                    parameters,
                    arrow.params.len(),
                    |visitor| {
                        arrow.body.visit_with(visitor);
                    },
                );
                arrow.body.visit_with(self);
            }
            _ => {}
        }
    }
}

struct HelperBodyVisitor<'a> {
    resolver: &'a Resolver,
    flow_index: &'a FlowIndex,
    parameters: Vec<String>,
    parameter_count: usize,
    sinks: Vec<HelperSink>,
}

impl HelperBodyVisitor<'_> {
    fn record_member_sink(&mut self, call: &CallExpr) {
        let Some(callee) = call_member_chain(call, self.resolver) else {
            return;
        };
        for (argument_index, argument) in call.args.iter().enumerate() {
            let Expr::Ident(argument_ident) = &*argument.expr else {
                continue;
            };
            let Some(param_index) = self
                .parameters
                .iter()
                .position(|parameter| parameter == argument_ident.sym.as_ref())
            else {
                continue;
            };
            for flow_id in self
                .flow_index
                .sinks
                .get(&callee)
                .into_iter()
                .flatten()
                .copied()
            {
                let Some(flow) = self.flow_index.get(flow_id) else {
                    continue;
                };
                if flow.sinks.iter().any(|sink| {
                    sink.member_calls
                        .iter()
                        .any(|member_call| member_call == &callee)
                        && match &sink.args {
                            FlowSinkArgs::Any => true,
                            FlowSinkArgs::Indices(indices) => indices.contains(&argument_index),
                        }
                }) {
                    self.sinks.push(HelperSink {
                        flow: flow_id,
                        param_index,
                        parameter_count: self.parameter_count,
                    });
                }
            }
        }
    }
}

impl Visit for HelperBodyVisitor<'_> {
    fn visit_call_expr(&mut self, call: &CallExpr) {
        if matches!(call.callee, Callee::Expr(ref callee) if matches!(&**callee, Expr::Member(_))) {
            self.record_member_sink(call);
        }
        call.visit_children_with(self);
    }
}

fn call_member_chain(call: &CallExpr, resolver: &Resolver) -> Option<String> {
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    member_callee_chain(callee, resolver)
}

fn member_callee_chain(expr: &Expr, resolver: &Resolver) -> Option<String> {
    resolver.rooted_expr_chain(expr).or_else(|| match expr {
        Expr::Member(member) => resolver
            .resolve_member(member)
            .rooted_chain
            .or_else(|| member_chain(member)),
        Expr::OptChain(chain) => match &*chain.base {
            OptChainBase::Member(member) => resolver
                .resolve_member(member)
                .rooted_chain
                .or_else(|| member_chain(member)),
            OptChainBase::Call(call) => member_callee_chain(&call.callee, resolver),
        },
        Expr::Paren(paren) => member_callee_chain(&paren.expr, resolver),
        Expr::Seq(sequence) => sequence
            .exprs
            .last()
            .and_then(|expr| member_callee_chain(expr, resolver)),
        _ => None,
    })
}

fn static_arg_matches(
    matcher: &ArgStringMatcher,
    args: &[ExprOrSpread],
    resolver: &Resolver,
) -> bool {
    args.get(matcher.index).is_some_and(|argument| {
        resolver
            .static_string_expr(&argument.expr)
            .is_some_and(|value| {
                matcher.predicate.as_ref().map_or_else(
                    || matcher.values.is_empty() || matcher.values.contains(&value),
                    |predicate| matches_static_value(predicate, &value),
                )
            })
    })
}

pub(super) fn matches_static_value(matcher: &FlowValueMatcher, value: &str) -> bool {
    match matcher {
        FlowValueMatcher::Any => true,
        FlowValueMatcher::StaticExact(values) => values.iter().any(|expected| expected == value),
        FlowValueMatcher::StaticPrefix(prefixes) => {
            prefixes.iter().any(|prefix| value.starts_with(prefix))
        }
        FlowValueMatcher::StaticContainsAny(markers) => {
            markers.iter().any(|marker| value.contains(marker))
        }
        FlowValueMatcher::StaticContainsAll(markers) => {
            markers.iter().all(|marker| value.contains(marker))
        }
    }
}

fn flow_value_matches(
    matcher: &FlowValueMatcher,
    static_value: Option<&str>,
    allow_dynamic_for_any: bool,
) -> bool {
    match matcher {
        FlowValueMatcher::Any => allow_dynamic_for_any || static_value.is_some(),
        FlowValueMatcher::StaticExact(values) => {
            static_value.is_some_and(|value| values.iter().any(|expected| expected == value))
        }
        FlowValueMatcher::StaticPrefix(prefixes) => static_value
            .is_some_and(|value| prefixes.iter().any(|prefix| value.starts_with(prefix))),
        FlowValueMatcher::StaticContainsAny(markers) => {
            static_value.is_some_and(|value| markers.iter().any(|marker| value.contains(marker)))
        }
        FlowValueMatcher::StaticContainsAll(markers) => {
            static_value.is_some_and(|value| markers.iter().all(|marker| value.contains(marker)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_tracking_when_the_configured_object_budget_is_exhausted() {
        let parsed = crate::parse(
            "const first = document.createElement('script'); first.src = url; document.head.appendChild(first); const second = document.createElement('script'); second.src = url; document.head.appendChild(second);",
            "flow-limit.js",
        )
        .expect("test source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let flow = FlowMatcher::new("script insertion")
            .source_member_call("document.createElement")
            .source_arg_string(0, ["script"])
            .property_write("src", FlowValueMatcher::Any)
            .sink_member_call_arg_indices(["document.head.appendChild"], [0]);
        let evidence = collect_with_limits(
            &parsed.program,
            &resolver,
            &[(0, 0, flow)],
            1,
            FlowLimits {
                max_objects: 1,
                max_states: 8,
                max_emissions: 8,
            },
        );

        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }
}
