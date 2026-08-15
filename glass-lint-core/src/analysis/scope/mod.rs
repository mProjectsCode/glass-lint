//! Lexical scopes plus the narrow alias facts needed by semantic matching.
//!
//! This is not a general JavaScript interpreter. It records only stable facts
//! that can be followed without speculation: imports, unshadowed globals,
//! direct aliases, selected static shapes, and prior assignments. Unknown or
//! mutable cases intentionally resolve to local/absent provenance.
//!
//! Collection is split into three phases:
//! 1. Declaration planning — all hoisted and block-scoped declarations are
//!    registered before any initializer is visited, so a use-before-decl
//!    resolves as local/TDZ rather than an unshadowed global.
//! 2. Source-order visitation — initializers, expressions, and nested scopes
//!    are visited in AST order.
//! 3. Freeze — the collected graph is sealed into an immutable query index.
//!
//! Binding IDs and assignment versions make position-sensitive queries
//! possible without rebuilding the AST for each lookup.

use build::{ScopeCollector, plan::ScopePlanner, traversal::ScopeTraversal};
use glass_lint_datastructures::NameTable;
use swc_common::Spanned;
use swc_ecma_ast::Program;
use swc_ecma_visit::VisitWith;

use crate::analysis::SemanticBudget;

mod binding_index;
mod build;
mod expression;
mod frozen_assignments;
mod graph;
mod mutation_index;
mod name_env;
mod query;
mod scope_index;
mod static_value;

pub(in crate::analysis) use build::program::{ScopeCollectionIssue, ScopedProgram};
pub(in crate::analysis) use expression::{ScopeExpression, normalize_scope_expression};
pub(in crate::analysis) use graph::{FrozenScopeGraph, ScopeGraph};
pub(in crate::analysis) use static_value::{const_value_to_provenance, provenance_to_const_value};

pub(in crate::analysis) use crate::analysis::model::scope::{
    AliasAssignment, BindingProvenance, BoundArgument, IdentValueSeed, LexicalScope, LexicalScopes,
    MemberValueSeed, ProvenanceAlternatives, ProvenanceJoin, ScopeEffect, ScopeId, ScopeKind,
    ScopedName,
};

impl ScopeGraph {
    #[cfg(test)]
    pub(super) fn collect(program: &Program) -> FrozenScopeGraph {
        let budget = SemanticBudget::default();
        let scoped = Self::collect_scoped_program(
            program,
            &crate::Environment::default(),
            NameTable::default(),
            &budget,
        );
        let ScopedProgram { graph, .. } = scoped;
        graph
    }

    pub(super) fn collect_scoped_program(
        program: &Program,
        environment: &crate::Environment,
        names: NameTable,
        budget: &SemanticBudget,
    ) -> ScopedProgram {
        let planner = ScopePlanner::new(program.span(), names, budget);
        let mut plan_traversal = ScopeTraversal::new(planner);
        program.visit_children_with(&mut plan_traversal);
        let plan = plan_traversal.into_pass().finish();

        let collector = ScopeCollector::from_plan(plan, budget);
        let mut collect_traversal = ScopeTraversal::new(collector);
        program.visit_children_with(&mut collect_traversal);
        collect_traversal.into_pass().freeze(environment)
    }
}

#[cfg(test)]
mod tests;
