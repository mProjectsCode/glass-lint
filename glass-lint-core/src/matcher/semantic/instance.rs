use swc_ecma_ast::{Callee, ClassDecl, ClassExpr, Expr};
use swc_ecma_visit::{Visit, VisitWith};

use super::super::rule::InstanceMemberCallMatcher;
use super::ast::member_prop_name;
use super::index::MatcherFacts;
use super::resolver::Resolver;

pub fn collect(
    program: &swc_ecma_ast::Program,
    resolver: &Resolver,
    matchers: &[InstanceMemberCallMatcher],
    index: &mut MatcherFacts,
) {
    if matchers.is_empty() {
        return;
    }
    let mut visitor = InstanceCollector {
        resolver,
        matchers,
        index,
        classes: Vec::new(),
        ordinary_functions: 0,
        static_methods: 0,
    };
    program.visit_with(&mut visitor);
}

struct InstanceCollector<'a> {
    resolver: &'a Resolver,
    matchers: &'a [InstanceMemberCallMatcher],
    index: &'a mut MatcherFacts,
    classes: Vec<Option<(String, String)>>,
    ordinary_functions: usize,
    static_methods: usize,
}

impl InstanceCollector<'_> {
    fn class_origin(&self) -> Option<&(String, String)> {
        self.classes.last().and_then(Option::as_ref)
    }

    fn record_call(&mut self, member: &swc_ecma_ast::MemberExpr, span: swc_common::Span) {
        if self.ordinary_functions != 0 || self.static_methods != 0 {
            return;
        }
        let receiver_is_this = matches!(&*member.obj, Expr::This(_))
            || self
                .resolver
                .resolve_expr(&member.obj)
                .rooted_chain
                .as_deref()
                .is_some_and(|chain| chain == "this");
        if !receiver_is_this {
            return;
        }
        let Some((module, export)) = self.class_origin().cloned() else {
            return;
        };
        let Some(member_name) = member_prop_name(&member.prop) else {
            return;
        };
        for matcher in self.matchers {
            if matcher.module == module && matcher.export == export && matcher.member == member_name
            {
                self.index
                    .instance_member_calls
                    .entry((module.clone(), export.clone(), member_name.clone()))
                    .or_default()
                    .push(span);
            }
        }
    }
}

impl Visit for InstanceCollector<'_> {
    fn visit_class_decl(&mut self, class: &ClassDecl) {
        let origin = class
            .class
            .super_class
            .as_deref()
            .and_then(|expr| self.resolver.class_provenance(expr));
        self.classes.push(origin);
        class.class.visit_children_with(self);
        self.classes.pop();
    }

    fn visit_class_expr(&mut self, class: &ClassExpr) {
        let origin = class
            .class
            .super_class
            .as_deref()
            .and_then(|expr| self.resolver.class_provenance(expr));
        self.classes.push(origin);
        class.class.visit_children_with(self);
        self.classes.pop();
    }

    fn visit_class_method(&mut self, method: &swc_ecma_ast::ClassMethod) {
        self.static_methods += usize::from(method.is_static);
        if let Some(body) = method.function.body.as_ref() {
            body.visit_with(self);
        }
        self.static_methods -= usize::from(method.is_static);
    }

    fn visit_call_expr(&mut self, call: &swc_ecma_ast::CallExpr) {
        if let Callee::Expr(callee) = &call.callee
            && let Expr::Member(member) = &**callee
        {
            self.record_call(member, call.span);
        }
        call.visit_children_with(self);
    }

    fn visit_fn_decl(&mut self, function: &swc_ecma_ast::FnDecl) {
        self.ordinary_functions += 1;
        function.visit_children_with(self);
        self.ordinary_functions -= 1;
    }

    fn visit_function(&mut self, function: &swc_ecma_ast::Function) {
        self.ordinary_functions += 1;
        function.visit_children_with(self);
        self.ordinary_functions -= 1;
    }
}
