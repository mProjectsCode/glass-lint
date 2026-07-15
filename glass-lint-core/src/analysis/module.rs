//! ApiMatcher-independent module requests and export interfaces.
//!
//! These records deliberately contain syntax-level names and source spans,
//! not matcher state or filesystem decisions.  The project linker turns the
//! request spans into public resolver keys after a source map is available.

use std::collections::{BTreeMap, BTreeSet};

use swc_common::Span;
use swc_ecma_ast::Pat;

use crate::{
    analysis::value::FunctionId,
    project::{ResolutionRequest, ResolutionRequestKey, ResolutionRequestKind},
};

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
    imported: Option<String>,
    local: String,
    namespace: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReExportBinding {
    imported: String,
    exported: String,
    namespace: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ModuleRequest {
    span: Span,
    kind: ResolutionRequestKind,
    specifier: String,
    role: ModuleRequestRole,
}

impl ImportedBinding {
    pub(crate) fn new(imported: Option<String>, local: String, namespace: bool) -> Self {
        Self {
            imported,
            local,
            namespace,
        }
    }

    pub(crate) fn imported(&self) -> Option<&str> {
        self.imported.as_deref()
    }

    pub(crate) fn is_namespace(&self) -> bool {
        self.namespace
    }
}

impl ReExportBinding {
    pub(crate) fn new(imported: String, exported: String, namespace: bool) -> Self {
        Self {
            imported,
            exported,
            namespace,
        }
    }
}

impl ModuleRequest {
    pub(crate) fn span(&self) -> Span {
        self.span
    }

    pub(crate) fn kind(&self) -> ResolutionRequestKind {
        self.kind
    }

    pub(crate) fn specifier(&self) -> &str {
        &self.specifier
    }

    pub(crate) fn role(&self) -> &ModuleRequestRole {
        &self.role
    }
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
    requests: Vec<ModuleRequest>,
    exports: BTreeMap<String, ModuleExport>,
    star_exports: Vec<usize>,
    locals: BTreeSet<String>,
    unknown_exports: bool,
    function_exports: BTreeMap<String, FunctionId>,
    static_strings: BTreeMap<String, String>,
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

    pub(in crate::analysis) fn add_function_export(
        &mut self,
        name: impl Into<String>,
        function: FunctionId,
    ) {
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

    pub(crate) fn add_static_string(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.static_strings.insert(name.into(), value.into());
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

    pub(crate) fn requests(&self) -> impl Iterator<Item = &ModuleRequest> {
        self.requests.iter()
    }

    pub(crate) fn request(&self, index: usize) -> Option<&ModuleRequest> {
        self.requests.get(index)
    }

    pub(crate) fn star_exports(&self) -> impl Iterator<Item = &usize> {
        self.star_exports.iter()
    }

    pub(crate) fn exports(&self) -> impl Iterator<Item = (&String, &ModuleExport)> {
        self.exports.iter()
    }

    pub(crate) fn is_local(&self, name: &str) -> bool {
        self.locals.contains(name)
    }

    pub(crate) fn is_unknown(&self) -> bool {
        self.unknown_exports
    }

    pub(crate) fn static_string(&self, name: &str) -> Option<&String> {
        self.static_strings.get(name)
    }

    pub(in crate::analysis) fn function_exports(&self) -> &BTreeMap<String, FunctionId> {
        &self.function_exports
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
                    kind: request.kind(),
                    range: crate::lint::source_range_from_span(source_map, request.span()),
                },
                request: request.specifier().to_owned(),
            })
            .collect()
    }
}
