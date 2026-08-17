use smol_str::ToSmolStr;
use swc_common::{Span, Spanned};
use swc_ecma_ast::NewExpr;

use crate::analysis::facts::{
    Expr, FactBuilder, FactPayload, Pat, SymbolCallProvenance, SymbolMemberProvenance,
    TargetProvenance, ValueId, VarDeclarator, VisitWith, effective_callee_expr,
    literal_member_property_name,
};

impl FactBuilder<'_, '_> {
    pub(super) fn record_new_expr(&mut self, new_expr: &NewExpr) {
        let result = self.resolver.fresh_object_value_at(new_expr.span).id;
        if let Some(instance_class) = self.constructor_origin_for_expr(&new_expr.callee) {
            self.provenance
                .record_instance_origin(result, instance_class, self.resolver.budget());
        }
        let effective_callee = effective_callee_expr(&new_expr.callee);
        let resolved = self.resolver.resolve_expr(effective_callee);
        let rooted_chain = resolved
            .provenance
            .rooted_chain
            .as_ref()
            .and_then(|path| self.name_path(path));
        let callee_span = effective_callee.span();

        let (callee_name, provenance) = match effective_callee {
            Expr::Ident(ident) => {
                let provenance = resolved.provenance.call.clone();
                (Some(ident.sym.to_smolstr()), provenance)
            }
            Expr::Member(member) => {
                let member_resolved = self.resolver.resolve_member(member);
                if let Some(SymbolMemberProvenance::ModuleNamespace {
                    ref module,
                    member: ref member_name,
                }) = member_resolved.provenance.module_member
                {
                    (
                        Some(member_name.clone()),
                        SymbolCallProvenance::ModuleExport {
                            module: module.clone(),
                            export: member_name.clone(),
                        },
                    )
                } else {
                    (
                        literal_member_property_name(&member.prop),
                        resolved.provenance.call.clone(),
                    )
                }
            }
            _ => (None, resolved.provenance.call.clone()),
        };

        new_expr.visit_children_with(self);
        let Some(callee_span) = self.byte_range(callee_span) else {
            return;
        };
        let callee_name = self.intern_name(callee_name.as_deref());
        self.emit(
            new_expr.span(),
            FactPayload::Construction {
                callee_span,
                callee_name,
                provenance,
                rooted_chain,
            },
        );
    }

    pub(super) fn declaration_source(&mut self, declarator: &VarDeclarator) -> ValueId {
        let source = declarator
            .init
            .as_ref()
            .map_or(ValueId::UNKNOWN, |init| self.value_for_expr(init));
        if let Some(init) = &declarator.init {
            init.visit_with(self);
        }
        if Self::is_simple_pattern(&declarator.name) {
            source
        } else {
            ValueId::UNKNOWN
        }
    }

    pub(super) fn declaration_targets(&mut self, pattern: &Pat) -> Vec<ValueId> {
        let mut targets = Vec::new();
        self.pattern_values(pattern, &mut targets);
        targets
    }

    pub(super) fn replace_declaration_provenance(
        &mut self,
        pattern: &Pat,
        init: Option<&Expr>,
        source: ValueId,
        targets: &[ValueId],
    ) {
        let replacement = if Self::is_simple_pattern(pattern) {
            init.map_or_else(TargetProvenance::default, |init| {
                self.target_provenance(init, source)
            })
        } else {
            TargetProvenance::default()
        };
        self.provenance
            .replace_targets(targets, &replacement, self.resolver.budget());
    }

    pub(super) fn emit_declarations(
        &mut self,
        span: Span,
        source: ValueId,
        mut targets: Vec<ValueId>,
    ) {
        if targets.is_empty() {
            targets.push(ValueId::UNKNOWN);
        }
        for target in targets {
            self.emit(span, FactPayload::Declaration { target, source });
        }
    }
}
