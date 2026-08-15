//! Conservative pattern projection walker shared by declaration and
//! assignment alias sinks.
//!
//! The walker emits an explicit unsupported/exhausted result instead of a
//! partial path.
//!
//! # Accepted patterns
//!
//! | Pattern form | Declaration | Assignment |
//! |---|---|---|
//! | `Pat::Ident` | project | project |
//! | Object `KeyValue` (static key) | recurse | recurse |
//! | Object `KeyValue` (computed static key) | recurse | recurse |
//! | Object shorthand `Assign` | project key | project key |
//! | `Pat::Assign` (default) | unsupported | unwrap left |
//! | `Pat::Rest` | unsupported | unsupported |
//! | Dynamic computed key | unsupported | unsupported |

use glass_lint_datastructures::NamePath;
use smol_str::{SmolStr, ToSmolStr};
use swc_ecma_ast::{AssignTargetPat, ObjectPatProp, Pat};

use crate::analysis::syntax::literal_property_name;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis::scope) enum ProjectionError {
    Unsupported,
    Exhausted,
}

/// A borrowed pattern input to the projection walker.
///
/// Declaration patterns and assignment targets have no common `&Pat` view, so
/// the walker borrows either shape instead of cloning an `AssignTargetPat`.
#[derive(Clone, Copy)]
pub(in crate::analysis::scope) enum BorrowedPattern<'a> {
    Decl(&'a Pat),
    Assign(&'a AssignTargetPat),
}

/// Walk a destructuring pattern and return projected (name, source-path) pairs.
///
/// Every projected binding receives the full source path from `base` through
/// the property chain that leads to its value in the initializer. The caller
/// supplies `append_segment` to extend a `NamePath` by one property name;
/// `None` means the name table is exhausted.
pub(in crate::analysis::scope) fn project_destructuring(
    pat: BorrowedPattern<'_>,
    base: &NamePath,
    is_assignment: bool,
    append_segment: &mut impl FnMut(&NamePath, &str) -> Option<NamePath>,
) -> Result<Vec<(SmolStr, NamePath)>, ProjectionError> {
    match pat {
        BorrowedPattern::Decl(pat) => project_pat(pat, base, is_assignment, append_segment),
        BorrowedPattern::Assign(AssignTargetPat::Object(object)) => {
            project_object(&object.props, base, is_assignment, append_segment)
        }
        BorrowedPattern::Assign(AssignTargetPat::Array(_) | AssignTargetPat::Invalid(_)) => {
            Err(ProjectionError::Unsupported)
        }
    }
}

fn project_pat(
    pat: &Pat,
    base: &NamePath,
    is_assignment: bool,
    append_segment: &mut impl FnMut(&NamePath, &str) -> Option<NamePath>,
) -> Result<Vec<(SmolStr, NamePath)>, ProjectionError> {
    match pat {
        Pat::Ident(ident) => Ok(vec![(ident.id.sym.to_smolstr(), base.clone())]),
        Pat::Assign(assign) if is_assignment => {
            project_pat(&assign.left, base, is_assignment, append_segment)
        }
        Pat::Object(object) => project_object(&object.props, base, is_assignment, append_segment),
        _ => Err(ProjectionError::Unsupported),
    }
}

fn project_object(
    props: &[ObjectPatProp],
    base: &NamePath,
    is_assignment: bool,
    append_segment: &mut impl FnMut(&NamePath, &str) -> Option<NamePath>,
) -> Result<Vec<(SmolStr, NamePath)>, ProjectionError> {
    let mut bindings = Vec::new();
    for prop in props {
        match prop {
            ObjectPatProp::KeyValue(kv) => {
                let key = literal_property_name(&kv.key).ok_or(ProjectionError::Unsupported)?;
                let child_base = append_segment(base, &key).ok_or(ProjectionError::Exhausted)?;
                bindings.extend(project_pat(
                    &kv.value,
                    &child_base,
                    is_assignment,
                    append_segment,
                )?);
            }
            ObjectPatProp::Assign(assign) => {
                let path = append_segment(base, assign.key.sym.as_ref())
                    .ok_or(ProjectionError::Exhausted)?;
                bindings.push((assign.key.sym.to_smolstr(), path));
            }
            ObjectPatProp::Rest(_) => return Err(ProjectionError::Unsupported),
        }
    }
    Ok(bindings)
}
