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

use swc_common::Span;
use swc_ecma_ast::{
    AssignExpr, AssignOp, AssignTarget, CallExpr, Callee, CondExpr, DoWhileStmt, Expr,
    ExprOrSpread, ForInStmt, ForOfStmt, ForStmt, IfStmt, MemberExpr, ObjectPatProp, OptChainBase,
    OptChainExpr, Pat, Program, SimpleAssignTarget, SwitchStmt, TryStmt, UnaryExpr, UnaryOp,
    UpdateExpr, VarDeclarator, WhileStmt,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::super::result::{ApiEvidence, ApiMatchKind};
use super::super::rule::{FlowMatcher, FlowRequirement, FlowSinkArgs};
use super::ast::member_prop_name;
use super::call::ResolvedCall;
use super::events::EventLog;
use super::flow_calls::{effective_flow_call, effective_opt_flow_call, flow_value_matches};
use super::flow_index::{FlowId, FlowIndex, FlowLimits};
use super::flow_state::{FlowState, state_is_ready};
use super::resolver::Resolver;
use super::value::{BindingKey, ObjectId};

pub fn collect(
    program: &Program,
    resolver: &Resolver,
    events: &EventLog,
    rules: &[(usize, usize, &FlowMatcher)],
    rule_count: usize,
) -> Vec<Vec<ApiEvidence>> {
    collect_with_limits(
        program,
        resolver,
        events,
        rules,
        rule_count,
        FlowLimits::default(),
    )
}

pub(super) fn collect_with_limits(
    program: &Program,
    resolver: &Resolver,
    events: &EventLog,
    rules: &[(usize, usize, &FlowMatcher)],
    rule_count: usize,
    limits: FlowLimits,
) -> Vec<Vec<ApiEvidence>> {
    let flow_index = FlowIndex::new(rules);
    let helpers = super::summary::FunctionSummaries {
        sinks: super::summary::collect(program, resolver, &flow_index),
    };
    let mut visitor = ObjectFlowCollector {
        resolver,
        events,
        flow_index,
        helpers,
        evidence: vec![Vec::new(); rule_count],
        aliases: BTreeMap::new(),
        states: BTreeMap::new(),
        emitted: BTreeSet::new(),
        next_object_id: 0,
        limits,
    };
    program.visit_with(&mut visitor);
    visitor.evidence
}

#[derive(Debug)]
struct ObjectFlowCollector<'resolver, 'rules> {
    resolver: &'resolver Resolver,
    events: &'resolver EventLog,
    flow_index: FlowIndex<'rules>,
    helpers: super::summary::FunctionSummaries,
    evidence: Vec<Vec<ApiEvidence>>,
    aliases: BTreeMap<BindingKey, ObjectId>,
    states: BTreeMap<(ObjectId, FlowId), FlowState>,
    emitted: BTreeSet<(usize, usize, ObjectId, u32, u32)>,
    next_object_id: u32,
    limits: FlowLimits,
}

impl ObjectFlowCollector<'_, '_> {
    fn expr_key(&self, expr: &Expr) -> Option<BindingKey> {
        self.resolver
            .binding_key_for_expr(expr)
            .or_else(|| self.resolver.binding_key_or_global(expr))
    }

    fn member_object_key(&self, member: &MemberExpr) -> Option<BindingKey> {
        self.expr_key(&member.obj)
    }

    fn source_invocation<'a>(&self, expression: &'a Expr) -> Option<(String, ResolvedCall<'a>)> {
        match expression {
            Expr::Call(call) => effective_flow_call(call, self.resolver),
            Expr::OptChain(chain) => match &*chain.base {
                OptChainBase::Call(call) => effective_opt_flow_call(call, self.resolver),
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
        call: &ResolvedCall<'_>,
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
                            && source.arg_strings.iter().all(|matcher| {
                                super::flow_calls::static_arg_matches(
                                    matcher,
                                    &call.args,
                                    self.resolver,
                                )
                            })
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
                source_span: call.span,
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
        if let Some((callee, call)) = self.source_invocation(value)
            && let Some((object_id, states)) = self.source_match(&callee, &call)
        {
            self.bind_states(key, object_id, states);
            return;
        }
        // Only copy a state through a direct identifier alias. Following
        // arbitrary expressions here would require control-flow and mutation
        // reasoning that this intentionally small analysis does not provide.
        if let Some(source_key) = self.expr_key(value)
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

    fn clear_target(&mut self, target: &Expr) {
        if let Some(key) = self.expr_key(target) {
            let object_id = match target {
                Expr::Member(member) => self
                    .member_object_key(member)
                    .and_then(|object_key| self.object_for_key(&object_key)),
                _ => None,
            };
            self.aliases.remove(&key);
            if let Some(object_id) = object_id {
                self.states.retain(|(object, _), _| *object != object_id);
            }
        }
    }

    fn allocate_object_id(&mut self) -> Option<ObjectId> {
        if self.next_object_id >= self.limits.max_objects {
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
        let Some((callee, effective_call)) = effective_flow_call(call, self.resolver) else {
            return;
        };
        for (argument_index, argument) in effective_call.args.iter().enumerate() {
            let Some(key) = self.expr_key(&argument.expr) else {
                continue;
            };
            self.emit_sink_matches(&key, &callee, argument_index);
        }
        if let Some(raw_callee) = super::ast::member_chain(member)
            && raw_callee != callee
        {
            for (argument_index, argument) in effective_call.args.iter().enumerate() {
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
                    .sinks
                    .get(&(
                        self.resolver.function_id_for_scope(scope),
                        callee.sym.to_string(),
                    ))
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
}

impl Visit for ObjectFlowCollector<'_, '_> {
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
        if self.events.order_for(declarator.span).is_none() {
            return;
        }
        if let (Pat::Ident(ident), Some(init)) = (&declarator.name, declarator.init.as_deref()) {
            self.assign_source_or_clear(&Expr::Ident(ident.id.clone()), init);
        } else {
            let mut targets = Vec::new();
            collect_pattern_targets(&declarator.name, &mut targets);
            for target in targets {
                self.clear_target(&target);
            }
        }
        declarator.visit_children_with(self);
    }

    fn visit_assign_expr(&mut self, assignment: &AssignExpr) {
        if self.events.order_for(assignment.span).is_none() {
            return;
        }
        if assignment.op != AssignOp::Assign {
            match &assignment.left {
                AssignTarget::Simple(SimpleAssignTarget::Ident(ident)) => {
                    self.clear_target(&Expr::Ident(ident.id.clone()));
                }
                AssignTarget::Simple(SimpleAssignTarget::Member(member)) => {
                    self.clear_target(&Expr::Member(member.clone()));
                }
                _ => {}
            }
            assignment.visit_children_with(self);
            return;
        }
        match &assignment.left {
            AssignTarget::Simple(SimpleAssignTarget::Ident(ident)) => {
                self.assign_source_or_clear(&Expr::Ident(ident.id.clone()), &assignment.right);
            }
            AssignTarget::Simple(SimpleAssignTarget::Member(member)) => {
                self.record_property_write(member, &assignment.right);
                self.assign_source_or_clear(&Expr::Member(member.clone()), &assignment.right);
            }
            AssignTarget::Pat(pattern) => {
                let pattern: Pat = pattern.clone().into();
                let mut targets = Vec::new();
                collect_pattern_targets(&pattern, &mut targets);
                for target in targets {
                    self.clear_target(&target);
                }
            }
            _ => {}
        }
        assignment.visit_children_with(self);
    }

    fn visit_update_expr(&mut self, update: &UpdateExpr) {
        if self.events.order_for(update.span).is_none() {
            return;
        }
        self.clear_target(&update.arg);
        update.visit_children_with(self);
    }

    fn visit_unary_expr(&mut self, unary: &UnaryExpr) {
        if self.events.order_for(unary.span).is_none() {
            return;
        }
        if unary.op == UnaryOp::Delete {
            self.clear_target(&unary.arg);
        }
        unary.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        if self.events.order_for(call.span).is_none() {
            return;
        }
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
}

fn collect_pattern_targets(pattern: &Pat, targets: &mut Vec<Expr>) {
    match pattern {
        Pat::Ident(ident) => targets.push(Expr::Ident(ident.id.clone())),
        Pat::Assign(assign) => collect_pattern_targets(&assign.left, targets),
        Pat::Rest(rest) => collect_pattern_targets(&rest.arg, targets),
        Pat::Array(array) => {
            for element in array.elems.iter().flatten() {
                collect_pattern_targets(element, targets);
            }
        }
        Pat::Object(object) => {
            for property in &object.props {
                match property {
                    ObjectPatProp::KeyValue(property) => {
                        collect_pattern_targets(&property.value, targets);
                    }
                    ObjectPatProp::Assign(property) => {
                        targets.push(Expr::Ident(property.key.id.clone()));
                    }
                    ObjectPatProp::Rest(rest) => collect_pattern_targets(&rest.arg, targets),
                }
            }
        }
        Pat::Expr(_) | Pat::Invalid(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::rule::FlowValueMatcher;

    #[test]
    fn stops_tracking_when_the_configured_object_budget_is_exhausted() {
        let parsed = crate::parse(
            "const first = document.createElement('script'); first.src = url; document.head.appendChild(first); const second = document.createElement('script'); second.src = url; document.head.appendChild(second);",
            "flow-limit.js",
        )
        .expect("test source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let events = EventLog::collect(&parsed.program).with_scopes(|_| 0);
        let flow = FlowMatcher::new("script insertion")
            .source_member_call("document.createElement")
            .source_arg_string(0, ["script"])
            .property_write("src", FlowValueMatcher::Any)
            .sink_member_call_arg_indices(["document.head.appendChild"], [0]);
        let evidence = collect_with_limits(
            &parsed.program,
            &resolver,
            &events,
            &[(0, 0, &flow)],
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
