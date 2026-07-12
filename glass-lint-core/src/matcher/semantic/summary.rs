//! Shared function-summary data used by semantic callback and flow analysis.
//!
//! A summary is keyed by the lexical scope that owns the function binding,
//! never by a process-wide function name.  The collector currently fills the
//! flow-sink projection here; the same shape is also the extension point for
//! callback parameters, writes, and return facts.

use std::collections::BTreeMap;

use swc_common::Spanned;
use swc_ecma_ast::{
    BlockStmt, BlockStmtOrExpr, CallExpr, Callee, Expr, FnDecl, ObjectPatProp, Pat, VarDeclarator,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::super::rule::FlowSinkArgs;
use super::ast::binding_ident_name;
use super::ast::prop_name;
use super::flow_index::{FlowId, FlowIndex};
use super::resolver::Resolver;
use super::scope::{AliasScope, BindingProvenance};
use super::value::FunctionId;

pub(super) type FunctionDeclarations = BTreeMap<(usize, String), (usize, Vec<Pat>)>;
pub(super) type FunctionInvocations = Vec<(usize, String, Vec<Option<BindingProvenance>>)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FunctionSinkSummary {
    pub(super) flow: FlowId,
    pub(super) param_index: usize,
    pub(super) parameter_count: usize,
}

#[derive(Debug, Default, Clone)]
pub(super) struct FunctionSummaries {
    pub(super) sinks: BTreeMap<(FunctionId, String), Vec<FunctionSinkSummary>>,
}

pub(super) fn parameter_aliases(
    functions: &FunctionDeclarations,
    calls: &FunctionInvocations,
    scopes: &[AliasScope],
) -> BTreeMap<(usize, String), BindingProvenance> {
    let mut aliases = BTreeMap::<(usize, String), Option<BindingProvenance>>::new();
    for (caller_scope, callee, arguments) in calls {
        let Some((scope, parameters)) = function_for_call(functions, scopes, *caller_scope, callee)
        else {
            continue;
        };
        for (index, parameter) in parameters.iter().enumerate() {
            let mut projected = BTreeMap::new();
            let recursive = *caller_scope == *scope;
            if !recursive && let Some(Some(target)) = arguments.get(index) {
                project_parameter_pattern(parameter, target, &mut projected);
            }
            // Every declared parameter must participate in the invocation
            // join.  Ignoring a missing argument (or an argument whose value
            // is dynamic) would let a compatible call contribute a strict
            // alias even though another invocation cannot establish it.
            for name in parameter_binding_names(parameter) {
                let target = projected.get(&name).cloned();
                let entry = aliases.entry((*scope, name)).or_insert(target.clone());
                if *entry != target {
                    *entry = None;
                }
            }
        }
        // Extra arguments are not projected into parameters.  They still
        // make this invocation incompatible with the summary's fixed
        // parameter shape, so invalidate every parameter alias for it.
        if arguments.len() != parameters.len() {
            for parameter in parameters {
                for name in parameter_binding_names(parameter) {
                    aliases.insert((*scope, name), None);
                }
            }
        }
    }
    aliases
        .into_iter()
        .filter_map(|(key, target)| target.map(|target| (key, target)))
        .collect()
}

fn parameter_binding_names(pattern: &Pat) -> Vec<String> {
    let mut names = Vec::new();
    collect_parameter_binding_names(pattern, &mut names);
    names.sort();
    names.dedup();
    names
}

fn collect_parameter_binding_names(pattern: &Pat, names: &mut Vec<String>) {
    match pattern {
        Pat::Ident(ident) => names.push(ident.id.sym.to_string()),
        Pat::Assign(assign) => collect_parameter_binding_names(&assign.left, names),
        Pat::Object(object) => {
            for property in &object.props {
                match property {
                    ObjectPatProp::KeyValue(property) => {
                        collect_parameter_binding_names(&property.value, names)
                    }
                    ObjectPatProp::Assign(property) => names.push(property.key.sym.to_string()),
                    ObjectPatProp::Rest(property) => {
                        collect_parameter_binding_names(&property.arg, names)
                    }
                }
            }
        }
        Pat::Array(array) => {
            for element in array.elems.iter().flatten() {
                collect_parameter_binding_names(element, names);
            }
        }
        Pat::Rest(rest) => collect_parameter_binding_names(&rest.arg, names),
        Pat::Expr(_) | Pat::Invalid(_) => {}
    }
}

fn function_for_call<'a>(
    functions: &'a FunctionDeclarations,
    scopes: &[AliasScope],
    mut scope: usize,
    name: &str,
) -> Option<&'a (usize, Vec<Pat>)> {
    loop {
        if let Some(function) = functions.get(&(scope, name.to_string())) {
            return Some(function);
        }
        scope = scopes[scope].parent?;
    }
}

pub(super) fn project_parameter_pattern(
    pattern: &Pat,
    value: &BindingProvenance,
    output: &mut BTreeMap<String, BindingProvenance>,
) {
    match pattern {
        Pat::Ident(ident) => {
            output.insert(ident.id.sym.to_string(), value.clone());
        }
        Pat::Assign(assign) => project_parameter_pattern(&assign.left, value, output),
        Pat::Object(object) => {
            let BindingProvenance::StaticObjectValues(values) = value else {
                return;
            };
            for property in &object.props {
                match property {
                    ObjectPatProp::KeyValue(property) => {
                        let Some(key) = prop_name(&property.key) else {
                            continue;
                        };
                        let Some(target) = values.get(&key) else {
                            continue;
                        };
                        project_parameter_pattern(
                            &property.value,
                            &BindingProvenance::ValueAlias {
                                target: target.clone(),
                            },
                            output,
                        );
                    }
                    ObjectPatProp::Assign(property) => {
                        if let Some(target) = values.get(property.key.sym.as_ref()) {
                            output.insert(
                                property.key.sym.to_string(),
                                BindingProvenance::ValueAlias {
                                    target: target.clone(),
                                },
                            );
                        }
                    }
                    ObjectPatProp::Rest(_) => {}
                }
            }
        }
        Pat::Array(_) | Pat::Rest(_) | Pat::Invalid(_) | Pat::Expr(_) => {}
    }
}

pub(super) fn collect<'rules>(
    program: &swc_ecma_ast::Program,
    resolver: &Resolver,
    flow_index: &'rules FlowIndex<'rules>,
) -> BTreeMap<(FunctionId, String), Vec<FunctionSinkSummary>> {
    let mut collector = FunctionSummaryCollector {
        resolver,
        flow_index,
        helpers: BTreeMap::new(),
    };
    program.visit_with(&mut collector);
    collector.helpers
}

struct FunctionSummaryCollector<'resolver, 'rules> {
    resolver: &'resolver Resolver,
    flow_index: &'rules FlowIndex<'rules>,
    helpers: BTreeMap<(FunctionId, String), Vec<FunctionSinkSummary>>,
}

impl FunctionSummaryCollector<'_, '_> {
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
        let mut visitor = FunctionBodySummary {
            resolver: self.resolver,
            flow_index: self.flow_index,
            parameters,
            parameter_count,
            sinks: Vec::new(),
        };
        body.visit_with(&mut visitor);
        self.helpers.insert(
            (self.resolver.function_id_for_scope(scope), name),
            visitor.sinks,
        );
    }

    fn record_arrow(
        &mut self,
        scope: usize,
        name: String,
        parameters: Vec<String>,
        parameter_count: usize,
        body: &BlockStmtOrExpr,
    ) {
        let mut visitor = FunctionBodySummary {
            resolver: self.resolver,
            flow_index: self.flow_index,
            parameters,
            parameter_count,
            sinks: Vec::new(),
        };
        body.visit_with(&mut visitor);
        self.helpers.insert(
            (self.resolver.function_id_for_scope(scope), name),
            visitor.sinks,
        );
    }
}

impl Visit for FunctionSummaryCollector<'_, '_> {
    fn visit_fn_decl(&mut self, function: &FnDecl) {
        let parameters = function
            .function
            .params
            .iter()
            .filter_map(|param| binding_ident_name(&param.pat))
            .collect::<Vec<_>>();
        let scope = self
            .resolver
            .scope_chain_at(
                function
                    .function
                    .body
                    .as_ref()
                    .map_or(function.ident.span, Spanned::span),
            )
            .get(2)
            .copied()
            .unwrap_or(0);
        self.record_function(
            scope,
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
                self.record_arrow(
                    self.resolver.scope_chain_at(ident.id.span)[0],
                    ident.id.sym.to_string(),
                    parameters,
                    arrow.params.len(),
                    &arrow.body,
                );
                arrow.body.visit_with(self);
            }
            _ => {}
        }
    }
}

struct FunctionBodySummary<'resolver, 'rules> {
    resolver: &'resolver Resolver,
    flow_index: &'rules FlowIndex<'rules>,
    parameters: Vec<String>,
    parameter_count: usize,
    sinks: Vec<FunctionSinkSummary>,
}

impl FunctionBodySummary<'_, '_> {
    fn record_member_sink(&mut self, call: &CallExpr) {
        let Some(callee) = super::flow_calls::call_member_chain(call, self.resolver) else {
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
                    self.sinks.push(FunctionSinkSummary {
                        flow: flow_id,
                        param_index,
                        parameter_count: self.parameter_count,
                    });
                }
            }
        }
    }
}

impl Visit for FunctionBodySummary<'_, '_> {
    fn visit_call_expr(&mut self, call: &CallExpr) {
        if matches!(call.callee, Callee::Expr(ref callee) if matches!(&**callee, Expr::Member(_))) {
            self.record_member_sink(call);
        }
        call.visit_children_with(self);
    }
}
