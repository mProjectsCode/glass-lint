//! AST naming, member-chain, and pattern helpers.
//!
//! Returned names are structural spellings, not proof of runtime identity.
//! Callers must combine them with scope/provenance queries before using a
//! chain for strict matching.

use std::collections::BTreeSet;

use glass_lint_datastructures::SymbolPath;
use smol_str::{SmolStr, ToSmolStr};
use swc_ecma_ast::{
    Callee, Expr, Ident, Lit, MemberExpr, MemberProp, ModuleExportName, ObjectPatProp,
    OptChainBase, Pat,
};

use crate::analysis::syntax::constant::{NoLookup, contextual_member_property_name};

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

/// Unwrap expression wrappers that preserve the inner expression's identity.
/// Empty sequences have no effective expression and therefore return `None`.
pub(in crate::analysis) fn unwrap_transparent_expr(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Paren(paren) => unwrap_transparent_expr(&paren.expr),
        Expr::Seq(sequence) => sequence
            .exprs
            .last()
            .and_then(|expr| unwrap_transparent_expr(expr)),
        Expr::TsAs(value) => unwrap_transparent_expr(&value.expr),
        Expr::TsNonNull(value) => unwrap_transparent_expr(&value.expr),
        Expr::TsSatisfies(value) => unwrap_transparent_expr(&value.expr),
        Expr::TsTypeAssertion(value) => unwrap_transparent_expr(&value.expr),
        _ => Some(expr),
    }
}

pub(in crate::analysis) fn is_dynamic_import(callee: &Callee) -> bool {
    matches!(callee, Callee::Import(_))
}

/// Walk every `Ident` binding introduced by a destructuring pattern.
/// The walker handles all standard JavaScript pattern forms (Ident, Assign,
/// Rest, Array, Object, Expr, Invalid) and calls `f` for each name.
fn walk_pat_ident_bindings(pat: &Pat, f: &mut impl FnMut(&Ident)) {
    match pat {
        Pat::Ident(ident) => f(&ident.id),
        Pat::Assign(assign) => walk_pat_ident_bindings(&assign.left, f),
        Pat::Rest(rest) => walk_pat_ident_bindings(&rest.arg, f),
        Pat::Array(array) => {
            for elem in array.elems.iter().flatten() {
                walk_pat_ident_bindings(elem, f);
            }
        }
        Pat::Object(object) => {
            for prop in &object.props {
                match prop {
                    ObjectPatProp::KeyValue(kv) => walk_pat_ident_bindings(&kv.value, f),
                    ObjectPatProp::Assign(assign) => f(&assign.key),
                    ObjectPatProp::Rest(rest) => walk_pat_ident_bindings(&rest.arg, f),
                }
            }
        }
        Pat::Invalid(_) | Pat::Expr(_) => {}
    }
}

/// Collect all names introduced by a binding pattern deterministically.
pub fn collect_pat_bindings(pat: &Pat, bindings: &mut BTreeSet<SmolStr>) {
    walk_pat_ident_bindings(pat, &mut |ident| {
        bindings.insert(ident.sym.to_smolstr());
    });
}

/// Normalize an identifier or string export name to its authored spelling.
pub fn module_export_name(name: &ModuleExportName) -> SmolStr {
    match name {
        ModuleExportName::Ident(ident) => ident.sym.to_smolstr(),
        ModuleExportName::Str(value) => value.value.to_string_lossy().to_smolstr(),
    }
}

/// Return a statically known object-literal property name.
///
/// This is a pure-syntax path distinct from the contextual property-name
/// conversion: it does not bound string keys with `MAX_STRING_BYTES`, accepts
/// arbitrary numeric keys (not only non-negative integers), and only resolves
/// computed keys that are literal strings. It must not be re-expressed through
/// the contextual evaluator, which would change these accepted shapes.
pub fn literal_property_name(name: &swc_ecma_ast::PropName) -> Option<SmolStr> {
    match name {
        swc_ecma_ast::PropName::Ident(ident) => Some(ident.sym.to_smolstr()),
        swc_ecma_ast::PropName::Str(value) => Some(value.value.to_string_lossy().to_smolstr()),
        swc_ecma_ast::PropName::Num(number) => Some(number.value.to_smolstr()),
        swc_ecma_ast::PropName::Computed(computed) => {
            if let Expr::Lit(Lit::Str(value)) = &*computed.expr {
                Some(value.value.to_string_lossy().to_smolstr())
            } else {
                None
            }
        }
        swc_ecma_ast::PropName::BigInt(_) => None,
    }
}

/// The effective terminal reached after walking through transparent shapes.
pub(in crate::analysis) enum TransparentTerminal<'a> {
    Expr(&'a Expr),
    Member(&'a MemberExpr),
}

/// Recurse through the expression shapes that are transparent to every caller
/// (call callee, optional-chain base, and parentheses) to the effective
/// terminal expression or member. Callers supply the identity step for that
/// terminal; shapes whose transparency differs between callers (sequences and
/// TypeScript assertion wrappers) remain terminal here.
pub(in crate::analysis) fn effective_terminal_expr(expr: &Expr) -> Option<TransparentTerminal<'_>> {
    match expr {
        Expr::Member(member) => Some(TransparentTerminal::Member(member)),
        Expr::Call(call) => {
            let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
                return None;
            };
            effective_terminal_expr(callee)
        }
        Expr::OptChain(chain) => match &*chain.base {
            OptChainBase::Member(member) => Some(TransparentTerminal::Member(member)),
            OptChainBase::Call(call) => effective_terminal_expr(&call.callee),
        },
        Expr::Paren(paren) => effective_terminal_expr(&paren.expr),
        _ => Some(TransparentTerminal::Expr(expr)),
    }
}

/// Render supported rooted expression shapes as a dotted syntax chain.
pub fn expression_name(expr: &Expr) -> Option<SymbolPath> {
    let terminal = effective_terminal_expr(expr)?;
    match terminal {
        TransparentTerminal::Expr(expr) => match expr {
            Expr::Ident(ident) => Some(SymbolPath::from(ident.sym.as_ref())),
            Expr::This(_) => Some(SymbolPath::from("this")),
            Expr::TsAs(value) => expression_name(&value.expr),
            Expr::TsNonNull(value) => expression_name(&value.expr),
            Expr::TsSatisfies(value) => expression_name(&value.expr),
            Expr::TsTypeAssertion(value) => expression_name(&value.expr),
            _ => None,
        },
        TransparentTerminal::Member(member) => member_expression_chain(member),
    }
}

/// Render a member expression as `object.property` when both parts are static.
pub fn member_expression_chain(member: &MemberExpr) -> Option<SymbolPath> {
    let mut properties = Vec::new();
    let mut expression = &member.obj;
    properties.push(literal_member_property_name(&member.prop)?);

    loop {
        match &**expression {
            Expr::Member(parent) => {
                properties.push(literal_member_property_name(&parent.prop)?);
                expression = &parent.obj;
            }
            Expr::Ident(ident) => {
                properties.reverse();
                let mut segments = vec![ident.sym.to_smolstr()];
                segments.extend(properties);
                return Some(SymbolPath::from_segments(segments));
            }
            Expr::This(_) => {
                properties.reverse();
                let mut segments = vec![SmolStr::from("this")];
                segments.extend(properties);
                return Some(SymbolPath::from_segments(segments));
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
pub fn literal_member_property_name(prop: &MemberProp) -> Option<SmolStr> {
    contextual_member_property_name(prop, &NoLookup)
}

/// Recognize a supported `Function`-like `.constructor` member shape.
pub fn is_function_constructor_member(member: &MemberExpr) -> bool {
    literal_member_property_name(&member.prop).as_deref() == Some("constructor")
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
    let chain = member_expression_chain(member)?;
    let builtin = if chain == SymbolPath::from("Object.getPrototypeOf") {
        "Object"
    } else if chain == SymbolPath::from("Reflect.getPrototypeOf") {
        "Reflect"
    } else {
        return None;
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

#[cfg(test)]
mod tests {
    use swc_ecma_ast::{Program, Stmt};

    use super::*;
    use crate::{parse::SourceLanguage, project::SourceFile};

    fn expression(source: &str) -> Expr {
        let parsed = crate::parse_test_source(&format!("{source};"), "names.js")
            .expect("test expression should parse");
        let Program::Script(script) = parsed.program else {
            panic!("test expression should parse as a script");
        };
        let Stmt::Expr(statement) = script.body.into_iter().next().unwrap() else {
            panic!("test source should contain one expression");
        };
        *statement.expr
    }

    fn ts_expression(source: &str) -> Expr {
        let source_text = format!("{source};");
        let source = SourceFile::with_language("names.ts", source_text, SourceLanguage::TypeScript)
            .expect("test source should be valid");
        let parsed = crate::parse::SourceParser::new(&source)
            .expect("test expression should parse")
            .parse()
            .expect("test expression should parse");
        let Program::Script(script) = parsed.program else {
            panic!("test expression should parse as a script");
        };
        let Stmt::Expr(statement) = script.body.into_iter().next().unwrap() else {
            panic!("test source should contain one expression");
        };
        *statement.expr
    }

    #[test]
    fn transparent_member_and_call_shapes_walk_to_the_same_terminal() {
        for source in [
            "a.b",
            "a.b()",
            "a.b(1, 2)",
            "a?.b",
            "a?.b()",
            "(a.b)",
            "((a.b))",
        ] {
            let expr = expression(source);
            let Some(TransparentTerminal::Member(member)) = effective_terminal_expr(&expr) else {
                panic!("{source} should terminate at a member");
            };
            assert_eq!(
                member_expression_chain(member).as_ref(),
                Some(&SymbolPath::from("a.b"))
            );
        }
        let expr = expression("a.b.c");
        let Some(TransparentTerminal::Member(member)) = effective_terminal_expr(&expr) else {
            panic!("a.b.c should terminate at a member");
        };
        assert_eq!(
            member_expression_chain(member).as_ref(),
            Some(&SymbolPath::from("a.b.c"))
        );
    }

    #[test]
    fn transparent_ident_and_this_shapes_walk_to_an_expr_terminal() {
        for source in ["a", "(a)", "this"] {
            let expr = expression(source);
            assert!(matches!(
                effective_terminal_expr(&expr),
                Some(TransparentTerminal::Expr(_))
            ));
        }
    }

    #[test]
    fn unsupported_callee_shapes_fail_closed() {
        let expr = expression("import('m')");
        assert!(effective_terminal_expr(&expr).is_none());
    }

    #[test]
    fn sequences_are_terminal_for_the_structural_name() {
        let expr = expression("(a, b)");
        assert!(matches!(
            effective_terminal_expr(&expr),
            Some(TransparentTerminal::Expr(Expr::Seq(_)))
        ));
        assert_eq!(expression_name(&expr), None);
    }

    #[test]
    fn expression_name_renders_supported_shapes() {
        assert_eq!(
            expression_name(&expression("a.b")),
            Some(SymbolPath::from("a.b"))
        );
        assert_eq!(
            expression_name(&expression("a.b()")),
            Some(SymbolPath::from("a.b"))
        );
        assert_eq!(
            expression_name(&expression("a?.b")),
            Some(SymbolPath::from("a.b"))
        );
        assert_eq!(
            expression_name(&expression("this.x")),
            Some(SymbolPath::from("this.x"))
        );
        assert_eq!(
            expression_name(&expression("a")),
            Some(SymbolPath::from("a"))
        );
    }

    #[test]
    fn ts_assertions_are_transparent_for_the_structural_name() {
        assert_eq!(
            expression_name(&ts_expression("a as T")),
            Some(SymbolPath::from("a"))
        );
        assert_eq!(
            expression_name(&ts_expression("(a.b) as T")),
            Some(SymbolPath::from("a.b"))
        );
    }
}
