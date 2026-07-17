//! AST naming, member-chain, and pattern helpers.
//!
//! Returned names are structural spellings, not proof of runtime identity.
//! Callers must combine them with scope/provenance queries before using a
//! chain for strict matching.

use std::collections::BTreeSet;

use swc_ecma_ast::{
    Expr, Ident, Lit, MemberExpr, MemberProp, ModuleExportName, ObjectPatProp, OptChainBase, Pat,
};

/// Find the lexical root identifier of a member/optional-chain expression.
pub fn member_root_identifier(member: &MemberExpr) -> Option<&Ident> {
    expr_root_ident(&member.obj)
}

fn expr_root_ident(expr: &Expr) -> Option<&Ident> {
    match expr {
        Expr::Ident(ident) => Some(ident),
        Expr::Member(parent) => member_root_identifier(parent),
        Expr::OptChain(chain) => match &*chain.base {
            OptChainBase::Member(member) => member_root_identifier(member),
            OptChainBase::Call(call) => expr_root_ident(&call.callee),
        },
        Expr::Paren(paren) => expr_root_ident(&paren.expr),
        _ => None,
    }
}

/// Strip transparent parentheses/sequences around a callee expression.
pub fn effective_callee_expr(expr: &Expr) -> &Expr {
    match expr {
        Expr::Paren(paren) => effective_callee_expr(&paren.expr),
        Expr::Seq(sequence) => sequence
            .exprs
            .last()
            .map_or(expr, |expr| effective_callee_expr(expr)),
        _ => expr,
    }
}

/// Collect all names introduced by a binding pattern deterministically.
pub fn collect_pat_bindings(pat: &Pat, bindings: &mut BTreeSet<String>) {
    match pat {
        Pat::Ident(ident) => {
            bindings.insert(ident.id.sym.to_string());
        }
        Pat::Array(array) => {
            for elem in array.elems.iter().flatten() {
                collect_pat_bindings(elem, bindings);
            }
        }
        Pat::Rest(rest) => collect_pat_bindings(&rest.arg, bindings),
        Pat::Object(object) => {
            for prop in &object.props {
                match prop {
                    ObjectPatProp::KeyValue(key_value) => {
                        collect_pat_bindings(&key_value.value, bindings);
                    }
                    ObjectPatProp::Assign(assign) => {
                        bindings.insert(assign.key.sym.to_string());
                    }
                    ObjectPatProp::Rest(rest) => collect_pat_bindings(&rest.arg, bindings),
                }
            }
        }
        Pat::Assign(assign) => collect_pat_bindings(&assign.left, bindings),
        Pat::Invalid(_) | Pat::Expr(_) => {}
    }
}

/// Normalize an identifier or string export name to its authored spelling.
pub fn module_export_name(name: &ModuleExportName) -> String {
    match name {
        ModuleExportName::Ident(ident) => ident.sym.to_string(),
        ModuleExportName::Str(value) => value.value.to_string_lossy().to_string(),
    }
}

/// Return a statically known object-literal property name.
pub fn property_name(name: &swc_ecma_ast::PropName) -> Option<String> {
    match name {
        swc_ecma_ast::PropName::Ident(ident) => Some(ident.sym.to_string()),
        swc_ecma_ast::PropName::Str(value) => Some(value.value.to_string_lossy().to_string()),
        swc_ecma_ast::PropName::Num(number) => Some(number.value.to_string()),
        swc_ecma_ast::PropName::Computed(computed) => {
            if let Expr::Lit(Lit::Str(value)) = &*computed.expr {
                Some(value.value.to_string_lossy().to_string())
            } else {
                None
            }
        }
        swc_ecma_ast::PropName::BigInt(_) => None,
    }
}

/// Render supported rooted expression shapes as a dotted syntax chain.
pub fn expression_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(ident) => Some(ident.sym.to_string()),
        Expr::Member(member) => member_expression_chain(member),
        Expr::Call(call) => {
            let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
                return None;
            };
            expression_name(callee)
        }
        Expr::This(_) => Some("this".to_string()),
        Expr::OptChain(chain) => match &*chain.base {
            OptChainBase::Member(member) => member_expression_chain(member),
            OptChainBase::Call(call) => expression_name(&call.callee),
        },
        Expr::Paren(paren) => expression_name(&paren.expr),
        Expr::TsAs(expr) => expression_name(&expr.expr),
        Expr::TsNonNull(expr) => expression_name(&expr.expr),
        Expr::TsSatisfies(expr) => expression_name(&expr.expr),
        Expr::TsTypeAssertion(expr) => expression_name(&expr.expr),
        _ => None,
    }
}

/// Render a member expression as `object.property` when both parts are static.
pub fn member_expression_chain(member: &MemberExpr) -> Option<String> {
    let mut properties = Vec::new();
    let mut expression = &member.obj;
    properties.push(member_property_name(&member.prop)?);
    loop {
        match &**expression {
            Expr::Member(parent) => {
                properties.push(member_property_name(&parent.prop)?);
                expression = &parent.obj;
            }
            Expr::Ident(ident) => {
                properties.reverse();
                let mut result = ident.sym.to_string();
                for property in properties {
                    result.push('.');
                    result.push_str(&property);
                }
                return Some(result);
            }
            Expr::This(_) => {
                properties.reverse();
                let mut result = String::from("this");
                for property in properties {
                    result.push('.');
                    result.push_str(&property);
                }
                return Some(result);
            }
            Expr::Call(call) => {
                let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
                    return None;
                };
                expression = callee;
            }
            Expr::Paren(paren) => expression = &paren.expr,
            Expr::TsAs(value) => expression = &value.expr,
            Expr::TsNonNull(value) => expression = &value.expr,
            Expr::TsSatisfies(value) => expression = &value.expr,
            Expr::TsTypeAssertion(value) => expression = &value.expr,
            _ => return None,
        }
    }
}

/// Return a statically known member property name, including private names.
pub fn member_property_name(prop: &MemberProp) -> Option<String> {
    match prop {
        MemberProp::Ident(ident) => Some(ident.sym.to_string()),
        MemberProp::PrivateName(name) => Some(format!("#{}", name.name)),
        MemberProp::Computed(computed) => static_property_name(&computed.expr),
    }
}

/// Recognize a supported `Function`-like `.constructor` member shape.
pub fn is_function_constructor_member(member: &MemberExpr) -> bool {
    member_property_name(&member.prop).as_deref() == Some("constructor")
        && is_function_like_expr(&member.obj)
}

/// Recognize one-argument `getPrototypeOf` calls on unqualified builtins.
pub fn function_prototype_builtin(expr: &Expr) -> Option<&'static str> {
    let Expr::Call(call) = expr else {
        return None;
    };
    let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(member) = &**callee else {
        return None;
    };
    let builtin = match member_expression_chain(member)?.as_str() {
        "Object.getPrototypeOf" => "Object",
        "Reflect.getPrototypeOf" => "Reflect",
        _ => return None,
    };
    (call.args.len() == 1 && is_function_like_expr(&call.args[0].expr)).then_some(builtin)
}

fn is_function_like_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Fn(_) | Expr::Arrow(_) => true,
        Expr::Call(_) => function_prototype_builtin(expr).is_some(),
        Expr::Paren(paren) => is_function_like_expr(&paren.expr),
        _ => false,
    }
}

fn static_property_name(expr: &Expr) -> Option<String> {
    super::constant::evaluate(expr, &super::constant::NoLookup).property_key()
}
