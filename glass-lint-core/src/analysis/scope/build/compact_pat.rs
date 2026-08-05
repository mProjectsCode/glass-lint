use std::collections::BTreeMap;

use smol_str::{SmolStr, ToSmolStr};
use swc_ecma_ast::{ObjectPatProp, Pat};

use crate::analysis::syntax::literal_property_name;

#[derive(Debug, Clone)]
pub enum CompactPat {
    Ident(SmolStr),
    Assign(Box<Self>),
    Object(BTreeMap<SmolStr, Self>),
    Array,
    Rest(Box<Self>),
    Other,
}

pub fn compact_pat(pattern: &Pat) -> CompactPat {
    match pattern {
        Pat::Ident(ident) => CompactPat::Ident(ident.id.sym.to_smolstr()),
        Pat::Assign(assign) => CompactPat::Assign(Box::new(compact_pat(&assign.left))),
        Pat::Object(object) => {
            let mut props = BTreeMap::new();
            for prop in &object.props {
                match prop {
                    ObjectPatProp::KeyValue(kv) => {
                        if let Some(key) = literal_property_name(&kv.key) {
                            props.insert(key, compact_pat(&kv.value));
                        }
                    }
                    ObjectPatProp::Assign(assign) => {
                        props.insert(
                            assign.key.sym.to_smolstr(),
                            CompactPat::Ident(assign.key.sym.to_smolstr()),
                        );
                    }
                    ObjectPatProp::Rest(_) => {}
                }
            }
            CompactPat::Object(props)
        }
        Pat::Array(_) => CompactPat::Array,
        Pat::Rest(rest) => CompactPat::Rest(Box::new(compact_pat(&rest.arg))),
        Pat::Invalid(_) | Pat::Expr(_) => CompactPat::Other,
    }
}
