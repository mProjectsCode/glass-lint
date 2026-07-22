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

use smol_str::{SmolStr, ToSmolStr};
use swc_ecma_ast::{ObjectPatProp, Pat};

use crate::analysis::{syntax::property_name, value::NamePath};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis::scope) enum ProjectionError {
    Unsupported,
    Exhausted,
}

/// Walk a destructuring pattern and return projected (name, source-path) pairs.
///
/// Every projected binding receives the full source path from `base` through
/// the property chain that leads to its value in the initializer. The caller
/// supplies `append_segment` to extend a `NamePath` by one property name;
/// `None` means the name table is exhausted.
pub(in crate::analysis::scope) fn project_destructuring(
    pat: &Pat,
    base: &NamePath,
    is_assignment: bool,
    append_segment: &impl Fn(&NamePath, &str) -> Option<NamePath>,
) -> Result<Vec<(SmolStr, NamePath)>, ProjectionError> {
    match pat {
        Pat::Ident(ident) => Ok(vec![(ident.id.sym.to_smolstr(), base.clone())]),
        Pat::Assign(assign) if is_assignment => {
            project_destructuring(&assign.left, base, is_assignment, append_segment)
        }
        Pat::Object(object) => {
            let mut bindings = Vec::new();
            for prop in &object.props {
                match prop {
                    ObjectPatProp::KeyValue(kv) => {
                        let key = property_name(&kv.key).ok_or(ProjectionError::Unsupported)?;
                        let child_base =
                            append_segment(base, &key).ok_or(ProjectionError::Exhausted)?;
                        bindings.extend(project_destructuring(
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
        _ => Err(ProjectionError::Unsupported),
    }
}
