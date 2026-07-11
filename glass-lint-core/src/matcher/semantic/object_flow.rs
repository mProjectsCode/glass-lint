//! Bounded, intra-function object flow for declarative flow matchers.
//!
//! A flow state begins at a configured source call, follows direct aliases,
//! accumulates configuration requirements, and is emitted only at a matching
//! sink. The analysis deliberately does not merge control-flow branches: that
//! conservative choice avoids turning an uncertain value into a report.

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

pub fn collect(
    program: &Program,
    resolver: &Resolver,
    rules: &[(usize, usize, FlowMatcher)],
    rule_count: usize,
) -> Vec<Vec<ApiEvidence>> {
    let helpers = HelperCollector::collect(program, resolver, rules);
    let mut visitor = ObjectFlowCollector {
        resolver,
        rules,
        helpers,
        evidence: vec![Vec::new(); rule_count],
        states: BTreeMap::new(),
        emitted: BTreeSet::new(),
        function_stack: vec![0],
        next_function_id: 1,
        next_object_id: 0,
    };
    program.visit_with(&mut visitor);
    visitor.evidence
}

#[derive(Debug, Clone)]
struct FlowState {
    rule_index: usize,
    flow_index: usize,
    source_span: Span,
    object_id: u32,
    requirements: BTreeSet<usize>,
    emitted: bool,
}

#[derive(Debug, Clone)]
struct HelperSink {
    rule_index: usize,
    flow_index: usize,
    param_index: usize,
}

struct ObjectFlowCollector<'a> {
    resolver: &'a Resolver,
    rules: &'a [(usize, usize, FlowMatcher)],
    helpers: BTreeMap<(usize, String), Vec<HelperSink>>,
    evidence: Vec<Vec<ApiEvidence>>,
    states: BTreeMap<String, Vec<FlowState>>,
    emitted: BTreeSet<(usize, usize, u32, u32, u32, u32)>,
    /// Flow keys are qualified by a synthetic function id. This prevents a
    /// reused local name in a nested or sibling function from inheriting state.
    function_stack: Vec<usize>,
    next_function_id: usize,
    next_object_id: u32,
}

impl ObjectFlowCollector<'_> {
    fn current_function(&self) -> usize {
        self.function_stack.last().copied().unwrap_or(0)
    }

    fn scoped_key(&self, chain: &str) -> String {
        format!("{}:{chain}", self.current_function())
    }

    fn expr_key(&self, expr: &Expr) -> Option<String> {
        self.resolver
            .rooted_expr_chain(expr)
            .map(|chain| self.scoped_key(&chain))
    }

    fn member_object_key(&self, member: &MemberExpr) -> Option<String> {
        self.expr_key(&member.obj)
    }

    fn source_match(&mut self, call: &CallExpr) -> Vec<FlowState> {
        let Some(callee) = call_member_chain(call, self.resolver) else {
            return Vec::new();
        };

        let matching =
            self.rules
                .iter()
                .filter(|(_, _, flow)| {
                    flow.sources.iter().any(|source| {
                        source.member_call == callee
                            && source.arg_strings.iter().all(|matcher| {
                                static_arg_matches(matcher, &call.args, self.resolver)
                            })
                    })
                })
                .map(|(rule_index, flow_index, _)| (*rule_index, *flow_index))
                .collect::<Vec<_>>();
        if matching.is_empty() {
            return Vec::new();
        }
        let object_id = self.next_object_id;
        self.next_object_id = self.next_object_id.saturating_add(1);
        matching
            .into_iter()
            .map(|(rule_index, flow_index)| FlowState {
                rule_index,
                flow_index,
                source_span: call.span,
                object_id,
                requirements: BTreeSet::new(),
                emitted: false,
            })
            .collect()
    }

    fn assign_source_or_clear(&mut self, target: &Expr, value: &Expr) {
        let Some(key) = self.expr_key(target) else {
            return;
        };
        if let Expr::Call(call) = value {
            let states = self.source_match(call);
            if !states.is_empty() {
                self.states.insert(key, states);
                return;
            }
        }
        // Only copy a state through a direct identifier alias. Following
        // arbitrary expressions here would require control-flow and mutation
        // reasoning that this intentionally small analysis does not provide.
        if matches!(value, Expr::Ident(_))
            && let Some(source_key) = self.expr_key(value)
            && let Some(states) = self.states.get(&source_key).cloned()
        {
            self.states.insert(key, states);
            return;
        }
        self.states.remove(&key);
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
        key: &str,
        mut matches_requirement: impl FnMut(&FlowMatcher, &FlowRequirement) -> bool,
    ) {
        let rules = self.rules;
        let Some(states) = self.states.get_mut(key) else {
            return;
        };
        let mut ready = Vec::new();
        for state in states.iter_mut() {
            let Some(flow) = flow_for_state(rules, state) else {
                continue;
            };
            for (requirement_index, requirement) in flow.requirements.iter().enumerate() {
                if matches_requirement(flow, requirement) {
                    state.requirements.insert(requirement_index);
                }
            }
            if flow.emit_on_requirements && state_is_ready(state, flow) && !state.emitted {
                state.emitted = true;
                ready.push((state.clone(), flow.clone()));
            }
        }
        for (state, flow) in ready {
            self.emit_state_if_ready(&state, &flow, state.source_span);
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
            self.emit_sink_matches(&key, &callee, argument_index, call.span);
        }
        if let Some(raw_callee) = member_chain(member)
            && raw_callee != callee
        {
            for (argument_index, argument) in call.args.iter().enumerate() {
                let Some(key) = self.expr_key(&argument.expr) else {
                    continue;
                };
                self.emit_sink_matches(&key, &raw_callee, argument_index, call.span);
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
            let Some(argument) = call.args.get(sink.param_index) else {
                continue;
            };
            let Some(key) = self.expr_key(&argument.expr) else {
                continue;
            };
            self.emit_flow_if_ready(&key, sink.rule_index, sink.flow_index, call.span);
        }
    }

    fn record_identifier_sink(&mut self, callee: &str, call: &CallExpr) {
        for (argument_index, argument) in call.args.iter().enumerate() {
            let Some(key) = self.expr_key(&argument.expr) else {
                continue;
            };
            self.emit_sink_matches(&key, callee, argument_index, call.span);
        }
    }

    fn record_sink_arguments(&mut self, callee: &str, args: &[ExprOrSpread], span: Span) {
        for (argument_index, argument) in args.iter().enumerate() {
            let Some(key) = self.expr_key(&argument.expr) else {
                continue;
            };
            self.emit_sink_matches(&key, callee, argument_index, span);
        }
    }

    fn emit_sink_matches(&mut self, key: &str, callee: &str, argument_index: usize, span: Span) {
        let Some(states) = self.states.get(key).cloned() else {
            return;
        };
        for state in states {
            let Some(flow) = flow_for_state(self.rules, &state) else {
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
                self.emit_state_if_ready(&state, flow, span);
            }
        }
    }

    fn emit_flow_if_ready(&mut self, key: &str, rule_index: usize, flow_index: usize, span: Span) {
        let Some(states) = self.states.get(key).cloned() else {
            return;
        };
        for state in states
            .iter()
            .filter(|state| state.rule_index == rule_index && state.flow_index == flow_index)
        {
            let Some(flow) = flow_for_state(self.rules, state) else {
                continue;
            };
            self.emit_state_if_ready(state, flow, span);
        }
    }

    fn emit_state_if_ready(&mut self, state: &FlowState, flow: &FlowMatcher, span: Span) {
        if !state_is_ready(state, flow) {
            return;
        }
        let key = (
            state.rule_index,
            state.flow_index,
            state.object_id,
            span.lo.0,
            span.hi.0,
            state.source_span.lo.0,
        );
        if self.emitted.insert(key) {
            // Flow evidence is anchored at the source allocation.  This keeps
            // the capability location stable for a flow that reaches several
            // sinks while the emission key still records the sink match site.
            self.emit_state(state, flow, state.source_span);
        }
    }

    fn emit_state(&mut self, state: &FlowState, flow: &FlowMatcher, span: Span) {
        self.evidence[state.rule_index].push(ApiEvidence {
            kind: ApiMatchKind::CallArgument,
            symbol: flow.evidence_symbol(),
            count: 1,
            spans: vec![span],
        });
    }

    fn enter_function(&mut self) {
        let id = self.next_function_id;
        self.next_function_id += 1;
        self.function_stack.push(id);
    }

    fn exit_function(&mut self) {
        self.function_stack.pop();
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
        statement.cons.visit_with(self);
        self.states = baseline.clone();
        if let Some(alternate) = &statement.alt {
            alternate.visit_with(self);
        }
        // A fact established in only one branch is not definite after the
        // join.  The branch-local visitors have already emitted valid
        // source-to-sink matches inside their own branch.
        self.states.clear();
    }

    fn visit_cond_expr(&mut self, expression: &CondExpr) {
        expression.test.visit_with(self);
        let baseline = self.states.clone();
        expression.cons.visit_with(self);
        self.states = baseline.clone();
        expression.alt.visit_with(self);
        self.states.clear();
    }

    fn visit_for_stmt(&mut self, statement: &ForStmt) {
        statement.init.visit_with(self);
        statement.test.visit_with(self);
        statement.update.visit_with(self);
        statement.body.visit_with(self);
        self.states.clear();
    }

    fn visit_for_in_stmt(&mut self, statement: &ForInStmt) {
        statement.left.visit_with(self);
        statement.right.visit_with(self);
        statement.body.visit_with(self);
        self.states.clear();
    }

    fn visit_for_of_stmt(&mut self, statement: &ForOfStmt) {
        statement.left.visit_with(self);
        statement.right.visit_with(self);
        statement.body.visit_with(self);
        self.states.clear();
    }

    fn visit_while_stmt(&mut self, statement: &WhileStmt) {
        statement.test.visit_with(self);
        statement.body.visit_with(self);
        self.states.clear();
    }

    fn visit_do_while_stmt(&mut self, statement: &DoWhileStmt) {
        statement.body.visit_with(self);
        statement.test.visit_with(self);
        self.states.clear();
    }

    fn visit_switch_stmt(&mut self, statement: &SwitchStmt) {
        statement.discriminant.visit_with(self);
        let baseline = self.states.clone();
        for case in &statement.cases {
            self.states = baseline.clone();
            case.visit_with(self);
        }
        self.states.clear();
    }

    fn visit_try_stmt(&mut self, statement: &TryStmt) {
        let baseline = self.states.clone();
        statement.block.visit_with(self);
        self.states = baseline.clone();
        statement.handler.visit_with(self);
        if let Some(finalizer) = &statement.finalizer {
            finalizer.visit_with(self);
        }
        self.states.clear();
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
            self.record_sink_arguments(&callee, &call.args, call.span);
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
    rules: &'a [(usize, usize, FlowMatcher)],
    helpers: BTreeMap<(usize, String), Vec<HelperSink>>,
}

impl<'a> HelperCollector<'a> {
    fn collect(
        program: &Program,
        resolver: &'a Resolver,
        rules: &'a [(usize, usize, FlowMatcher)],
    ) -> BTreeMap<(usize, String), Vec<HelperSink>> {
        let mut collector = Self {
            resolver,
            rules,
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
        body: Option<&BlockStmt>,
    ) {
        let Some(body) = body else {
            return;
        };
        self.record_helper(scope, name, parameters, |visitor| body.visit_with(visitor));
    }

    fn record_helper(
        &mut self,
        scope: usize,
        name: String,
        parameters: Vec<String>,
        visit_body: impl FnOnce(&mut HelperBodyVisitor<'_>),
    ) {
        let mut visitor = HelperBodyVisitor {
            resolver: self.resolver,
            rules: self.rules,
            parameters,
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
    rules: &'a [(usize, usize, FlowMatcher)],
    parameters: Vec<String>,
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
            for (rule_index, flow_index, flow) in self.rules {
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
                        rule_index: *rule_index,
                        flow_index: *flow_index,
                        param_index,
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
    let Expr::Member(member) = &**callee else {
        return None;
    };
    resolver
        .resolve_member(member)
        .rooted_chain
        .or_else(|| member_chain(member))
}

fn flow_for_state<'a>(
    rules: &'a [(usize, usize, FlowMatcher)],
    state: &FlowState,
) -> Option<&'a FlowMatcher> {
    rules
        .iter()
        .find(|(rule_index, flow_index, _)| {
            *rule_index == state.rule_index && *flow_index == state.flow_index
        })
        .map(|(_, _, flow)| flow)
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
