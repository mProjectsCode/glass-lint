//! Matcher-independent module requests and export interfaces.
//!
//! These records deliberately contain syntax-level names and source spans,
//! not matcher state or filesystem decisions.  The project linker turns the
//! request spans into public resolver keys after a source map is available.

use std::collections::{BTreeMap, BTreeSet};

use swc_common::Span;
use swc_ecma_ast::Pat;

use crate::analysis::value::FunctionId;
use crate::project::{ResolutionRequest, ResolutionRequestKey, ResolutionRequestKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ModuleRequestRole {
    Import { bindings: Vec<ImportedBinding> },
    ReExport { bindings: Vec<ReExportBinding> },
    StarExport,
    DynamicImport,
    Require,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImportedBinding {
    pub(crate) imported: Option<String>,
    pub(crate) local: String,
    pub(crate) namespace: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReExportBinding {
    pub(crate) imported: String,
    pub(crate) exported: String,
    pub(crate) namespace: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ModuleRequest {
    pub(crate) span: Span,
    pub(crate) kind: ResolutionRequestKind,
    pub(crate) specifier: String,
    pub(crate) role: ModuleRequestRole,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ModuleExport {
    Local { name: String },
    Value,
    ReExport { request: usize, imported: String },
    Namespace { request: usize },
    Unknown,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ModuleInterface {
    pub(crate) requests: Vec<ModuleRequest>,
    pub(crate) exports: BTreeMap<String, ModuleExport>,
    pub(crate) star_exports: Vec<usize>,
    pub(crate) locals: BTreeSet<String>,
    pub(crate) unknown_exports: bool,
    pub(crate) function_exports: BTreeMap<String, FunctionId>,
}

impl ModuleInterface {
    pub(crate) fn add_local(&mut self, name: impl Into<String>) {
        self.locals.insert(name.into());
    }

    pub(crate) fn add_pattern_locals(&mut self, pattern: &Pat) {
        let mut names = BTreeSet::new();
        crate::analysis::syntax::collect_pat_bindings(pattern, &mut names);
        self.locals.extend(names);
    }

    pub(crate) fn add_request(
        &mut self,
        span: Span,
        kind: ResolutionRequestKind,
        specifier: impl Into<String>,
        role: ModuleRequestRole,
    ) -> usize {
        let index = self.requests.len();
        self.requests.push(ModuleRequest {
            span,
            kind,
            specifier: specifier.into(),
            role,
        });
        index
    }

    pub(crate) fn add_export(&mut self, name: impl Into<String>, export: ModuleExport) {
        if self.unknown_exports {
            return;
        }
        let name = name.into();
        match self.exports.get(&name) {
            None => {
                self.exports.insert(name, export);
            }
            Some(existing) if existing == &export => {}
            Some(_) => {
                self.exports.insert(name, ModuleExport::Unknown);
            }
        }
    }

    pub(crate) fn add_function_export(&mut self, name: impl Into<String>, function: FunctionId) {
        let name = name.into();
        match self.function_exports.get(&name) {
            None => {
                self.function_exports.insert(name, function);
            }
            Some(existing) if *existing == function => {}
            Some(_) => {
                self.function_exports.remove(&name);
            }
        }
    }

    pub(crate) fn add_star_export(&mut self, request: usize) {
        if !self.unknown_exports {
            self.star_exports.push(request);
        }
    }

    pub(crate) fn mark_unknown_exports(&mut self) {
        self.exports.clear();
        self.star_exports.clear();
        self.unknown_exports = true;
    }

    pub(crate) fn has_exports(&self) -> bool {
        !self.exports.is_empty() || !self.star_exports.is_empty()
    }

    pub(crate) fn authored_requests(
        &self,
        importer: &str,
        source_map: &swc_common::sync::Lrc<swc_common::SourceMap>,
    ) -> Vec<ResolutionRequest> {
        self.requests
            .iter()
            .map(|request| ResolutionRequest {
                key: ResolutionRequestKey {
                    importer: importer.to_string(),
                    kind: request.kind,
                    range: crate::lint::source_range_from_span(source_map, request.span),
                },
                request: request.specifier.clone(),
            })
            .collect()
    }
}
