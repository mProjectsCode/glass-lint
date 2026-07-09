use std::collections::{BTreeMap, BTreeSet};

use swc_common::Span;
use swc_ecma_ast::{
    AssignExpr, AssignTarget, BlockStmt, CallExpr, Callee, Expr, ExprOrSpread, FnDecl, Function,
    MemberExpr, Pat, Program, SimpleAssignTarget, VarDeclarator,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::alias::AliasInfo;
use super::ast::{binding_ident_name, member_chain, member_prop_name};
use super::{ApiEvidence, ApiMatchKind};
use crate::matcher::rule::{
    ArgStringMatcher, FlowSinkArgs, FlowValueMatcher, ValueFlowConfiguration, ValueFlowMatcher,
};

pub fn collect(
    program: &Program,
    aliases: &AliasInfo,
    rules: &[(usize, usize, ValueFlowMatcher)],
    rule_count: usize,
) -> Vec<Vec<ApiEvidence>> {
    let helpers = HelperCollector::collect(program, aliases, rules);
    let mut visitor = ValueFlowVisitor {
        aliases,
        rules,
        helpers,
        evidence: vec![Vec::new(); rule_count],
        states: BTreeMap::new(),
        function_stack: vec![0],
        next_function_id: 1,
    };
    program.visit_with(&mut visitor);
    visitor.evidence
}

#[derive(Debug, Clone)]
struct FlowState {
    rule_index: usize,
    flow_index: usize,
    source_span: Span,
    configurations: BTreeSet<usize>,
}

#[derive(Debug, Clone)]
struct HelperSink {
    rule_index: usize,
    flow_index: usize,
    param_index: usize,
}

struct ValueFlowVisitor<'a> {
    aliases: &'a AliasInfo,
    rules: &'a [(usize, usize, ValueFlowMatcher)],
    helpers: BTreeMap<String, Vec<HelperSink>>,
    evidence: Vec<Vec<ApiEvidence>>,
    states: BTreeMap<String, Vec<FlowState>>,
    function_stack: Vec<usize>,
    next_function_id: usize,
}

impl ValueFlowVisitor<'_> {
    fn current_function(&self) -> usize {
        *self
            .function_stack
            .last()
            .expect("program function scope is always present")
    }

    fn scoped_key(&self, chain: &str) -> String {
        format!("{}:{chain}", self.current_function())
    }

    fn expr_key(&self, expr: &Expr) -> Option<String> {
        self.aliases
            .rooted_expr_chain(expr)
            .map(|chain| self.scoped_key(&chain))
    }

    fn member_object_key(&self, member: &MemberExpr) -> Option<String> {
        self.expr_key(&member.obj)
    }

    fn source_match(&self, call: &CallExpr) -> Vec<FlowState> {
        let Some(callee) = call_member_chain(call, self.aliases) else {
            return Vec::new();
        };

        self.rules
            .iter()
            .filter(|(_, _, flow)| {
                flow.sources.iter().any(|source| {
                    source.member_call == callee
                        && source
                            .arg_strings
                            .iter()
                            .all(|matcher| static_arg_matches(matcher, &call.args, self.aliases))
                })
            })
            .map(|(rule_index, flow_index, _)| FlowState {
                rule_index: *rule_index,
                flow_index: *flow_index,
                source_span: call.span,
                configurations: BTreeSet::new(),
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
        self.states.remove(&key);
    }

    fn record_property_write(&mut self, member: &MemberExpr, value: &Expr) {
        let Some(key) = self.member_object_key(member) else {
            return;
        };
        let Some(property) = member_prop_name(&member.prop) else {
            return;
        };
        let static_value = self.aliases.static_string_expr(value);
        self.update_configurations(&key, |_flow, configuration| match configuration {
            ValueFlowConfiguration::PropertyWrite {
                property: expected,
                value,
            } => expected == &property && flow_value_matches(value, static_value.as_deref(), true),
            ValueFlowConfiguration::MemberCall { .. } => false,
        });
    }

    fn record_member_configuration(&mut self, member: &MemberExpr, call: &CallExpr) {
        let Some(key) = self.member_object_key(member) else {
            return;
        };
        let Some(called_member) = member_prop_name(&member.prop) else {
            return;
        };
        self.update_configurations(&key, |_flow, configuration| match configuration {
            ValueFlowConfiguration::MemberCall { member, args } => {
                member == &called_member
                    && args.iter().all(|matcher| {
                        call.args.get(matcher.index).is_some_and(|arg| {
                            let value = self.aliases.static_string_expr(&arg.expr);
                            flow_value_matches(&matcher.value, value.as_deref(), true)
                        })
                    })
            }
            ValueFlowConfiguration::PropertyWrite { .. } => false,
        });
    }

    fn update_configurations(
        &mut self,
        key: &str,
        mut matches_configuration: impl FnMut(&ValueFlowMatcher, &ValueFlowConfiguration) -> bool,
    ) {
        let rules = self.rules;
        let Some(states) = self.states.get_mut(key) else {
            return;
        };
        for state in states {
            let Some(flow) = flow_for_state(rules, state) else {
                continue;
            };
            for (configuration_index, configuration) in flow.configurations.iter().enumerate() {
                if matches_configuration(flow, configuration) {
                    state.configurations.insert(configuration_index);
                }
            }
        }
    }

    fn record_member_sink(&mut self, member: &MemberExpr, call: &CallExpr) {
        let Some(callee) = call_member_chain(call, self.aliases) else {
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

    fn record_helper_sink(&mut self, callee: &str, call: &CallExpr) {
        let Some(sinks) = self.helpers.get(callee).cloned() else {
            return;
        };
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

    fn emit_state_if_ready(&mut self, state: &FlowState, flow: &ValueFlowMatcher, _span: Span) {
        let ready = if flow.all_configurations_required {
            state.configurations.len() == flow.configurations.len()
        } else {
            !state.configurations.is_empty()
        };
        if !ready {
            return;
        }
        self.evidence[state.rule_index].push(ApiEvidence {
            kind: ApiMatchKind::CallArgument,
            symbol: flow.evidence_symbol(),
            count: 1,
            spans: vec![state.source_span],
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

impl Visit for ValueFlowVisitor<'_> {
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
                    self.record_helper_sink(ident.sym.as_ref(), call);
                }
                _ => {}
            },
            Callee::Super(_) | Callee::Import(_) => {}
        }
        call.visit_children_with(self);
    }

    fn visit_function(&mut self, function: &Function) {
        self.enter_function();
        function.visit_children_with(self);
        self.exit_function();
    }
}

struct HelperCollector<'a> {
    aliases: &'a AliasInfo,
    rules: &'a [(usize, usize, ValueFlowMatcher)],
    helpers: BTreeMap<String, Vec<HelperSink>>,
}

impl<'a> HelperCollector<'a> {
    fn collect(
        program: &Program,
        aliases: &'a AliasInfo,
        rules: &'a [(usize, usize, ValueFlowMatcher)],
    ) -> BTreeMap<String, Vec<HelperSink>> {
        let mut collector = Self {
            aliases,
            rules,
            helpers: BTreeMap::new(),
        };
        program.visit_with(&mut collector);
        collector.helpers
    }

    fn record_function(&mut self, name: String, parameters: Vec<String>, body: Option<&BlockStmt>) {
        let Some(body) = body else {
            return;
        };
        self.record_helper(name, parameters, |visitor| body.visit_with(visitor));
    }

    fn record_helper(
        &mut self,
        name: String,
        parameters: Vec<String>,
        visit_body: impl FnOnce(&mut HelperBodyVisitor<'_>),
    ) {
        let mut visitor = HelperBodyVisitor {
            aliases: self.aliases,
            rules: self.rules,
            parameters,
            sinks: Vec::new(),
        };
        visit_body(&mut visitor);
        if !visitor.sinks.is_empty() {
            self.helpers.insert(name, visitor.sinks);
        }
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
            function.ident.sym.to_string(),
            parameters,
            function.function.body.as_ref(),
        );
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
                    ident.id.sym.to_string(),
                    parameters,
                    function.function.body.as_ref(),
                );
            }
            Expr::Arrow(arrow) => {
                let parameters = arrow
                    .params
                    .iter()
                    .filter_map(binding_ident_name)
                    .collect::<Vec<_>>();
                self.record_helper(ident.id.sym.to_string(), parameters, |visitor| {
                    arrow.body.visit_with(visitor);
                });
            }
            _ => {}
        }
    }
}

struct HelperBodyVisitor<'a> {
    aliases: &'a AliasInfo,
    rules: &'a [(usize, usize, ValueFlowMatcher)],
    parameters: Vec<String>,
    sinks: Vec<HelperSink>,
}

impl HelperBodyVisitor<'_> {
    fn record_member_sink(&mut self, call: &CallExpr) {
        let Some(callee) = call_member_chain(call, self.aliases) else {
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

fn call_member_chain(call: &CallExpr, aliases: &AliasInfo) -> Option<String> {
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(member) = &**callee else {
        return None;
    };
    aliases
        .rooted_member_chain(member)
        .or_else(|| member_chain(member))
}

fn flow_for_state<'a>(
    rules: &'a [(usize, usize, ValueFlowMatcher)],
    state: &FlowState,
) -> Option<&'a ValueFlowMatcher> {
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
    aliases: &AliasInfo,
) -> bool {
    args.get(matcher.index).is_some_and(|argument| {
        aliases
            .static_string_expr(&argument.expr)
            .is_some_and(|value| matcher.values.is_empty() || matcher.values.contains(&value))
    })
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
