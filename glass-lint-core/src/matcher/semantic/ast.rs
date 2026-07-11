use std::collections::BTreeSet;

use swc_ecma_ast::{
    BinaryOp, Expr, Ident, Lit, MemberExpr, MemberProp, ModuleExportName, ObjectLit, ObjectPatProp,
    OptChainBase, Pat, Prop, PropOrSpread,
};

pub fn member_root_ident(member: &MemberExpr) -> Option<&Ident> {
    expr_root_ident(&member.obj)
}

fn expr_root_ident(expr: &Expr) -> Option<&Ident> {
    match expr {
        Expr::Ident(ident) => Some(ident),
        Expr::Member(parent) => member_root_ident(parent),
        Expr::OptChain(chain) => match &*chain.base {
            OptChainBase::Member(member) => member_root_ident(member),
            OptChainBase::Call(call) => expr_root_ident(&call.callee),
        },
        Expr::Paren(paren) => expr_root_ident(&paren.expr),
        _ => None,
    }
}

pub fn expr_member(expr: &Expr) -> Option<&MemberExpr> {
    match expr {
        Expr::Member(member) => Some(member),
        Expr::OptChain(chain) => match &*chain.base {
            OptChainBase::Member(member) => Some(member),
            OptChainBase::Call(call) => expr_member(&call.callee),
        },
        Expr::Paren(paren) => expr_member(&paren.expr),
        _ => None,
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolCallProvenance {
    Global { name: String },
    Local,
    ModuleExport { module: String, export: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolMemberProvenance {
    ModuleNamespace { module: String, member: String },
}

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

pub fn binding_ident_name(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(ident) => Some(ident.id.sym.to_string()),
        Pat::Assign(assign) => binding_ident_name(&assign.left),
        _ => None,
    }
}

pub fn module_export_name(name: &ModuleExportName) -> String {
    match name {
        ModuleExportName::Ident(ident) => ident.sym.to_string(),
        ModuleExportName::Str(value) => value.value.to_string_lossy().to_string(),
    }
}

pub fn prop_name(name: &swc_ecma_ast::PropName) -> Option<String> {
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

pub fn expr_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(ident) => Some(ident.sym.to_string()),
        Expr::Member(member) => member_chain(member),
        Expr::Call(call) => {
            let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
                return None;
            };
            expr_name(callee)
        }
        Expr::This(_) => Some("this".to_string()),
        Expr::OptChain(chain) => match &*chain.base {
            OptChainBase::Member(member) => member_chain(member),
            OptChainBase::Call(call) => expr_name(&call.callee),
        },
        Expr::Paren(paren) => expr_name(&paren.expr),
        Expr::TsAs(expr) => expr_name(&expr.expr),
        Expr::TsNonNull(expr) => expr_name(&expr.expr),
        Expr::TsSatisfies(expr) => expr_name(&expr.expr),
        Expr::TsTypeAssertion(expr) => expr_name(&expr.expr),
        _ => None,
    }
}

pub fn member_chain(member: &MemberExpr) -> Option<String> {
    let object = expr_name(&member.obj)?;
    let prop = member_prop_name(&member.prop)?;
    Some(format!("{object}.{prop}"))
}

pub fn member_prop_name(prop: &MemberProp) -> Option<String> {
    match prop {
        MemberProp::Ident(ident) => Some(ident.sym.to_string()),
        MemberProp::PrivateName(name) => Some(format!("#{}", name.name)),
        MemberProp::Computed(computed) => static_property_name(&computed.expr),
    }
}

pub fn object_keys(object: &ObjectLit) -> Option<Vec<String>> {
    let mut keys = Vec::new();
    for property in &object.props {
        let key = match property {
            PropOrSpread::Prop(property) => match &**property {
                Prop::Shorthand(ident) => ident.sym.to_string(),
                Prop::KeyValue(key_value) => prop_name(&key_value.key)?,
                Prop::Method(method) => prop_name(&method.key)?,
                Prop::Getter(getter) => prop_name(&getter.key)?,
                Prop::Setter(setter) => prop_name(&setter.key)?,
                Prop::Assign(assign) => assign.key.sym.to_string(),
            },
            PropOrSpread::Spread(_) => return None,
        };
        keys.push(key);
    }
    Some(keys)
}

pub fn is_function_constructor_member(member: &MemberExpr) -> bool {
    member_prop_name(&member.prop).as_deref() == Some("constructor")
        && is_function_like_expr(&member.obj)
}

fn is_function_like_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Fn(_) | Expr::Arrow(_) => true,
        Expr::Call(call) => {
            let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
                return false;
            };
            matches!(&**callee, Expr::Member(member)
                if member_chain(member).as_deref() == Some("Object.getPrototypeOf"))
                && call.args.len() == 1
                && is_function_like_expr(&call.args[0].expr)
        }
        Expr::Paren(paren) => is_function_like_expr(&paren.expr),
        _ => false,
    }
}

fn static_property_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(Lit::Str(value)) => Some(value.value.to_string_lossy().to_string()),
        Expr::Lit(Lit::Num(value)) => Some(value.value.to_string()),
        Expr::Tpl(template) if template.exprs.is_empty() && template.quasis.len() == 1 => {
            template.quasis.first().map(|quasi| quasi.raw.to_string())
        }
        Expr::Bin(binary) if binary.op == BinaryOp::Add => Some(format!(
            "{}{}",
            static_property_name(&binary.left)?,
            static_property_name(&binary.right)?
        )),
        Expr::Paren(paren) => static_property_name(&paren.expr),
        _ => None,
    }
}

pub fn static_string(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(Lit::Str(value)) => Some(value.value.to_string_lossy().to_string()),
        Expr::Tpl(template) if template.exprs.is_empty() && template.quasis.len() == 1 => {
            template.quasis.first().map(|quasi| quasi.raw.to_string())
        }
        Expr::Paren(paren) => static_string(&paren.expr),
        _ => None,
    }
}
