use std::collections::BTreeMap;

use smol_str::{SmolStr, ToSmolStr};
use swc_common::Spanned;
use swc_ecma_ast::{
    BinExpr, Expr, Ident, Lit, MemberExpr, MemberProp, ObjectLit, Prop, PropName, PropOrSpread, Tpl,
};

use crate::analysis::syntax::constant::types::{
    self, ConstValue, MAX_ARRAY_ITEMS, MAX_DEPTH, MAX_LOOKUPS, MAX_NODES, MAX_OBJECT_KEYS,
    MAX_STRING_BYTES, merge_bounded,
};

pub(in crate::analysis) trait Lookup {
    /// Resolve an identifier through the caller's lexical model.
    fn ident(&self, ident: &Ident, state: &mut EvalState) -> ConstValue;
    /// Resolve a member through the caller's lexical model.
    ///
    /// The default implementation evaluates only statically named array/object
    /// members: numeric indices distinguish array elements from object keys,
    /// computed properties are resolved through the same property-name helper,
    /// and every unsupported case (dynamic names, missing members,
    /// non-constant receivers, exhausted values) returns `Unknown`.
    fn member(&self, member: &MemberExpr, state: &mut EvalState) -> ConstValue
    where
        Self: Sized,
    {
        let Some(property) = contextual_member_property_name_with_state(&member.prop, self, state)
        else {
            return ConstValue::Unknown;
        };
        match state.evaluate(member.obj.as_ref(), self) {
            ConstValue::Array(values) => property
                .parse::<usize>()
                .ok()
                .and_then(|index| values.get(index).cloned())
                .unwrap_or(ConstValue::Unknown),
            ConstValue::Object(values) => values
                .get(&property)
                .cloned()
                .unwrap_or(ConstValue::Unknown),
            _ => ConstValue::Unknown,
        }
    }
    /// Check whether a global name is unshadowed at a source span.
    fn unshadowed_global(&self, name: &str, span: swc_common::Span) -> bool;

    /// Spreading a mutable object is intentionally weaker than passing that
    /// object directly: a later mutation or reassignment can change the
    /// copied shape before the use site.
    fn spread(&self, expr: &Expr, state: &mut EvalState) -> ConstValue
    where
        Self: Sized,
    {
        state.evaluate(expr, self)
    }
}

#[derive(Debug, Default, Clone, Copy)]
/// Lookup implementation that intentionally resolves no identifiers.
pub(in crate::analysis) struct NoLookup;

impl Lookup for NoLookup {
    fn ident(&self, _ident: &Ident, _state: &mut EvalState) -> ConstValue {
        ConstValue::Unknown
    }

    fn member(&self, _member: &MemberExpr, _state: &mut EvalState) -> ConstValue {
        ConstValue::Unknown
    }

    fn unshadowed_global(&self, _name: &str, _span: swc_common::Span) -> bool {
        false
    }
}

/// Evaluate one expression under the evaluator's fresh bounded state.
pub(in crate::analysis) fn evaluate<'a>(
    node: impl Into<EvalNode<'a>>,
    lookup: &impl Lookup,
) -> ConstValue {
    let mut state = EvalState::default();
    state.evaluate(node, lookup)
}

/// Evaluate a member property with a fresh bounded state.
pub(in crate::analysis) fn contextual_member_property_name(
    prop: &MemberProp,
    lookup: &impl Lookup,
) -> Option<SmolStr> {
    let mut state = EvalState::default();
    contextual_member_property_name_with_state(prop, lookup, &mut state)
}

/// Evaluate a member property while sharing an existing evaluation budget.
pub(in crate::analysis) fn contextual_member_property_name_with_state(
    prop: &MemberProp,
    lookup: &impl Lookup,
    state: &mut EvalState,
) -> Option<SmolStr> {
    state.contextual_member_property_name(prop, lookup)
}

#[derive(Default)]
/// Mutable recursion/node/lookup budget for one constant evaluation.
pub(in crate::analysis) struct EvalState {
    /// Current recursive expression depth.
    depth: usize,
    /// Number of visited expression/container nodes.
    nodes: usize,
    /// Number of identifier/member lookups performed.
    lookups: usize,
}

/// Borrowed syntax input accepted by the bounded evaluator.
pub(in crate::analysis) enum EvalNode<'a> {
    Expr(&'a Expr),
    Binary(&'a BinExpr),
    Template(&'a Tpl),
}

impl<'a> From<&'a Expr> for EvalNode<'a> {
    fn from(expr: &'a Expr) -> Self {
        Self::Expr(expr)
    }
}

impl<'a> From<&'a BinExpr> for EvalNode<'a> {
    fn from(binary: &'a BinExpr) -> Self {
        Self::Binary(binary)
    }
}

impl<'a> From<&'a Tpl> for EvalNode<'a> {
    fn from(template: &'a Tpl) -> Self {
        Self::Template(template)
    }
}

impl EvalState {
    /// Evaluate one expression, failing closed when any bound is exhausted.
    pub(in crate::analysis) fn evaluate<'a>(
        &mut self,
        node: impl Into<EvalNode<'a>>,
        lookup: &impl Lookup,
    ) -> ConstValue {
        if !self.consume_node() {
            return ConstValue::Unknown;
        }
        let node = node.into();
        let value = self.evaluate_inner(&node, lookup);
        self.depth -= 1;
        value
    }

    /// Charge the node/depth budget for a nested child node, returning
    /// whether evaluation may proceed. Mirrors the accounting `evaluate`
    /// applies to a wrapped node.
    fn consume_node(&mut self) -> bool {
        if self.depth >= MAX_DEPTH || self.nodes >= MAX_NODES {
            return false;
        }
        self.nodes += 1;
        self.depth += 1;
        true
    }

    // Kept as a single dispatch match: each arm delegates to a focused helper
    // or returns directly. Splitting the match would scatter related Expr cases.
    fn evaluate_inner(&mut self, node: &EvalNode<'_>, lookup: &impl Lookup) -> ConstValue {
        // Single match over every Expr variant: each arm is self-contained and
        // extracting them would add boilerplate without improving clarity.
        let expr = match node {
            EvalNode::Expr(expr) => *expr,
            EvalNode::Binary(binary) => {
                return if binary.op == swc_ecma_ast::BinaryOp::Add {
                    self.evaluate_add(binary, lookup)
                } else {
                    ConstValue::Unknown
                };
            }
            EvalNode::Template(template) => return self.evaluate_template(template, lookup),
        };
        match expr {
            Expr::Lit(Lit::Str(value)) => {
                ConstValue::bounded_string(value.value.to_string_lossy().to_string())
            }
            Expr::Lit(Lit::Num(value)) => types::non_negative_integer(value.value)
                .map_or(ConstValue::Unknown, ConstValue::NonNegativeInteger),
            Expr::Ident(ident) => self.lookup_ident(lookup, ident),
            Expr::Member(member) => self.lookup_member(lookup, member),
            Expr::Paren(paren) => self.evaluate(paren.expr.as_ref(), lookup),
            Expr::Seq(sequence) => sequence.exprs.last().map_or(ConstValue::Unknown, |expr| {
                self.evaluate(expr.as_ref(), lookup)
            }),
            Expr::Assign(assign) => self.evaluate(assign.right.as_ref(), lookup),
            Expr::Bin(binary) if binary.op == swc_ecma_ast::BinaryOp::Add => {
                self.evaluate_add(binary, lookup)
            }
            Expr::Tpl(template) => self.evaluate_template(template, lookup),
            Expr::Array(array) if array.elems.len() <= MAX_ARRAY_ITEMS => {
                let mut values = Vec::with_capacity(array.elems.len());
                for element in &array.elems {
                    let Some(element) = element else {
                        return ConstValue::Unknown;
                    };
                    values.push(self.evaluate(element.expr.as_ref(), lookup));
                }
                ConstValue::array(values)
            }
            Expr::Object(object) => self.evaluate_object(object, lookup),
            Expr::Call(call) => self.evaluate_object_assign(call, lookup),
            Expr::TsAs(value) => self.evaluate(value.expr.as_ref(), lookup),
            Expr::TsNonNull(value) => self.evaluate(value.expr.as_ref(), lookup),
            Expr::TsSatisfies(value) => self.evaluate(value.expr.as_ref(), lookup),
            Expr::TsTypeAssertion(value) => self.evaluate(value.expr.as_ref(), lookup),
            _ => ConstValue::Unknown,
        }
    }

    fn evaluate_template(&mut self, template: &Tpl, lookup: &impl Lookup) -> ConstValue {
        let mut output = String::new();
        for (index, quasi) in template.quasis.iter().enumerate() {
            let cooked = quasi.cooked.as_ref().map_or_else(
                || quasi.raw.to_string(),
                |value| value.to_string_lossy().to_string(),
            );
            if !Self::append_bounded(&mut output, &cooked) {
                return ConstValue::Unknown;
            }
            if let Some(expression) = template.exprs.get(index) {
                let Some(value) = self
                    .evaluate(expression.as_ref(), lookup)
                    .to_property_string()
                else {
                    return ConstValue::Unknown;
                };
                if !Self::append_bounded(&mut output, &value) {
                    return ConstValue::Unknown;
                }
            }
        }
        ConstValue::String(output)
    }

    fn evaluate_add(&mut self, binary: &BinExpr, lookup: &impl Lookup) -> ConstValue {
        let left = self.evaluate(binary.left.as_ref(), lookup);
        let right = self.evaluate(binary.right.as_ref(), lookup);
        match (&left, &right) {
            (ConstValue::NonNegativeInteger(left), ConstValue::NonNegativeInteger(right)) => left
                .checked_add(*right)
                .map_or(ConstValue::Unknown, ConstValue::NonNegativeInteger),
            (ConstValue::String(_), _) | (_, ConstValue::String(_)) => {
                let Some(left) = left.to_property_string() else {
                    return ConstValue::Unknown;
                };
                let Some(right) = right.to_property_string() else {
                    return ConstValue::Unknown;
                };
                let mut value = left;
                if !Self::append_bounded(&mut value, &right) {
                    return ConstValue::Unknown;
                }
                ConstValue::String(value)
            }
            _ => ConstValue::Unknown,
        }
    }

    fn evaluate_object(&mut self, object: &ObjectLit, lookup: &impl Lookup) -> ConstValue {
        if object.props.len() > MAX_OBJECT_KEYS {
            return ConstValue::Unknown;
        }
        let mut values = BTreeMap::new();
        for property in &object.props {
            match property {
                PropOrSpread::Spread(spread) => {
                    let ConstValue::Object(spread_values) = lookup.spread(&spread.expr, self)
                    else {
                        return ConstValue::Unknown;
                    };
                    if !merge_bounded(&mut values, spread_values) {
                        return ConstValue::Unknown;
                    }
                }
                PropOrSpread::Prop(property) => {
                    let (key, value) = match &**property {
                        Prop::Shorthand(ident) => {
                            (ident.sym.to_smolstr(), self.evaluate_ident(lookup, ident))
                        }
                        Prop::KeyValue(property) => {
                            let Some(key) = self.contextual_property_name(&property.key, lookup)
                            else {
                                return ConstValue::Unknown;
                            };
                            (key, self.evaluate(property.value.as_ref(), lookup))
                        }
                        _ => return ConstValue::Unknown,
                    };
                    values.insert(key, value);
                }
            }
        }
        ConstValue::object(values)
    }

    fn evaluate_object_assign(
        &mut self,
        call: &swc_ecma_ast::CallExpr,
        lookup: &impl Lookup,
    ) -> ConstValue {
        let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
            return ConstValue::Unknown;
        };
        let Expr::Member(member) = &**callee else {
            return ConstValue::Unknown;
        };
        if contextual_member_property_name_with_state(&member.prop, lookup, self).as_deref()
            != Some("assign")
            || !matches!(&*member.obj, Expr::Ident(ident) if ident.sym == *"Object")
            || !lookup.unshadowed_global("Object", member.obj.span())
            || call.args.is_empty()
        {
            return ConstValue::Unknown;
        }
        let mut values = BTreeMap::new();
        for argument in &call.args {
            let ConstValue::Object(argument_values) = self.evaluate(argument.expr.as_ref(), lookup)
            else {
                return ConstValue::Unknown;
            };
            if !merge_bounded(&mut values, argument_values) {
                return ConstValue::Unknown;
            }
        }
        ConstValue::object(values)
    }

    fn lookup_ident(&mut self, lookup: &impl Lookup, ident: &Ident) -> ConstValue {
        if !self.consume_lookup() {
            return ConstValue::Unknown;
        }
        lookup.ident(ident, self)
    }

    fn evaluate_ident(&mut self, lookup: &impl Lookup, ident: &Ident) -> ConstValue {
        if !self.consume_node() {
            return ConstValue::Unknown;
        }
        let value = self.lookup_ident(lookup, ident);
        self.depth -= 1;
        value
    }

    fn lookup_member(&mut self, lookup: &impl Lookup, member: &MemberExpr) -> ConstValue {
        if !self.consume_lookup() {
            return ConstValue::Unknown;
        }
        lookup.member(member, self)
    }

    fn consume_lookup(&mut self) -> bool {
        if self.lookups >= MAX_LOOKUPS {
            return false;
        }
        self.lookups += 1;
        true
    }

    /// Resolve a property name using the same bounded evaluator state as its
    /// surrounding expression. Computed keys therefore consume depth, node,
    /// and lookup budget instead of silently starting a second evaluation.
    fn contextual_property_name(
        &mut self,
        prop: &PropName,
        lookup: &impl Lookup,
    ) -> Option<SmolStr> {
        match prop {
            PropName::Ident(ident) => Some(ident.sym.to_smolstr()),
            PropName::Str(value) => {
                ConstValue::bounded_string(value.value.to_string_lossy().to_string()).property_key()
            }
            PropName::Num(value) => {
                types::non_negative_integer(value.value).map(|value| value.to_smolstr())
            }
            PropName::BigInt(_) => None,
            PropName::Computed(computed) => {
                self.evaluate(computed.expr.as_ref(), lookup).property_key()
            }
        }
    }

    fn contextual_member_property_name(
        &mut self,
        prop: &MemberProp,
        lookup: &impl Lookup,
    ) -> Option<SmolStr> {
        match prop {
            MemberProp::Ident(ident) => Some(ident.sym.to_smolstr()),
            MemberProp::PrivateName(name) => Some(format!("#{}", name.name).to_smolstr()),
            MemberProp::Computed(computed) => {
                self.evaluate(computed.expr.as_ref(), lookup).property_key()
            }
        }
    }

    fn append_bounded(output: &mut String, value: &str) -> bool {
        if output.len().saturating_add(value.len()) > MAX_STRING_BYTES {
            return false;
        }
        output.push_str(value);
        true
    }
}
