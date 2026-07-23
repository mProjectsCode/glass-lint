//! Public-matcher-independent module requests and export interfaces.
//!
//! These records deliberately contain syntax-level names and source byte
//! ranges, not matcher state or filesystem decisions. The project linker turns
//! the request byte ranges into public resolver keys after a source map is
//! available.
//!
//! Dynamic or conflicting export shapes are retained as explicit unknown
//! state. The project linker can therefore distinguish “not exported” from
//! “exported but not safely resolvable.”

use std::collections::{BTreeMap, BTreeSet};

use smol_str::SmolStr;

use crate::{
    ByteRange,
    analysis::value::FunctionId,
    project::{
        ProjectRelativePath, ResolutionRequest, ResolutionRequestKey, ResolutionRequestKind,
    },
};

pub const DEFAULT_EXPORT: &str = "default";
pub const NAMESPACE_EXPORT: &str = "*";
pub const COMMONJS_MODULE: &str = "module";
pub const COMMONJS_EXPORTS: &str = "exports";
pub const COMMONJS_REQUIRE: &str = "require";

#[derive(Clone, Debug, Eq, PartialEq)]
/// Why a module request exists and which runtime bindings it introduces.
pub enum ModuleRequestRole {
    /// Static ESM import and its local bindings.
    Import { bindings: Vec<ImportedBinding> },
    /// Re-export bindings sourced from another module.
    ReExport { bindings: Vec<ReExportBinding> },
    /// `export * from` request.
    StarExport,
    /// Literal dynamic `import()` request.
    DynamicImport,
    /// Literal CommonJS `require()` request.
    Require,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// One import binding with optional imported name and namespace semantics.
pub struct ImportedBinding {
    /// Exported name, or `None` for namespace imports.
    imported: Option<SmolStr>,
    /// Local binding introduced in the importer.
    local: SmolStr,
    /// Whether the binding represents the complete namespace.
    namespace: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// One named/default/namespace binding exposed through a re-export.
pub struct ReExportBinding {
    /// Name read from the source module.
    imported: SmolStr,
    /// Name exposed by the current module.
    exported: SmolStr,
    /// Whether the exported binding is a namespace.
    namespace: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Authored module request before filesystem resolution.
pub struct ModuleRequest {
    id: ModuleRequestId,
    /// Source span of the literal specifier.
    span: ByteRange,
    /// Resolver classification requested by the syntax.
    kind: ResolutionRequestKind,
    /// Literal module specifier as authored.
    specifier: SmolStr,
    /// Import/export role associated with the request.
    role: ModuleRequestRole,
}

impl ImportedBinding {
    /// Construct an imported binding with optional namespace semantics.
    pub fn new(imported: Option<SmolStr>, local: SmolStr, namespace: bool) -> Self {
        Self {
            imported,
            local,
            namespace,
        }
    }

    /// Return the source export name, if one was specified.
    pub fn imported(&self) -> Option<&SmolStr> {
        self.imported.as_ref()
    }

    /// Whether this binding refers to the whole module namespace.
    pub fn is_namespace(&self) -> bool {
        self.namespace
    }
}

impl ReExportBinding {
    /// Construct a re-export binding.
    pub fn new(imported: SmolStr, exported: SmolStr, namespace: bool) -> Self {
        Self {
            imported,
            exported,
            namespace,
        }
    }
}

impl ModuleRequest {
    /// Stable identity within the owning module interface.
    pub fn id(&self) -> ModuleRequestId {
        self.id
    }

    /// Return the literal specifier span.
    pub fn span(&self) -> ByteRange {
        self.span
    }

    /// Return the syntax-derived request kind.
    pub fn kind(&self) -> ResolutionRequestKind {
        self.kind
    }

    /// Return the authored module specifier.
    pub fn specifier(&self) -> &SmolStr {
        &self.specifier
    }

    /// Return the import/export role metadata.
    pub fn role(&self) -> &ModuleRequestRole {
        &self.role
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Export shape recorded by the local module pass.
pub enum ModuleExport {
    /// Export aliases a local binding.
    Local { name: SmolStr },
    /// Export exists but is represented by a non-callable value identity.
    Value,
    /// Export is forwarded through a specific request.
    ReExport {
        request: ModuleRequestId,
        imported: SmolStr,
    },
    /// Export exposes a namespace from a request.
    Namespace { request: ModuleRequestId },
    /// Export shape is ambiguous or unsupported.
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Resolution, function identity, and static-string value for one export name.
///
/// All three pieces of metadata share the same lifecycle: conflicts and unknown
/// export shapes degrade them atomically so consumers cannot observe stale
/// function or string data after the export resolution has been invalidated.
pub struct ExportEntry {
    /// The export shape (local, re-export, namespace, unknown).
    ///
    /// `None` when only function or static metadata has been recorded but the
    /// export resolution itself has not yet been set. Once set, conflicts and
    /// unknown degradation clear the optional metadata fields.
    pub(super) resolution: Option<ModuleExport>,
    /// Function identity, if the export resolves to a function.
    pub(super) function_id: Option<FunctionId>,
    /// Statically known string value, if the export is a string constant.
    pub(super) static_value: Option<String>,
}

impl ExportEntry {
    fn new(resolution: ModuleExport) -> Self {
        Self {
            resolution: Some(resolution),
            function_id: None,
            static_value: None,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// Matcher-independent imports, exports, locals, and static exported values.
pub struct ModuleInterface {
    requests: Vec<ModuleRequest>,
    requests_by_specifier: BTreeMap<SmolStr, Vec<ModuleRequestId>>,
    exports: BTreeMap<SmolStr, ExportEntry>,
    star_exports: Vec<ModuleRequestId>,
    locals: BTreeSet<SmolStr>,
    unknown_exports: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
/// Stable identity of a request authored by one module interface.
pub struct ModuleRequestId(usize);

impl ModuleRequestId {
    fn index(self) -> usize {
        self.0
    }
}

impl ModuleInterface {
    /// Record a local binding name for module-boundary checks.
    pub fn add_local(&mut self, name: impl Into<SmolStr>) {
        self.locals.insert(name.into());
    }

    /// Append one authored module request and return its stable local index.
    pub fn add_request(
        &mut self,
        span: ByteRange,
        kind: ResolutionRequestKind,
        specifier: impl Into<SmolStr>,
        role: ModuleRequestRole,
    ) -> ModuleRequestId {
        let index = ModuleRequestId(self.requests.len());
        self.requests.push(ModuleRequest {
            id: index,
            span,
            kind,
            specifier: specifier.into(),
            role,
        });
        self.requests_by_specifier
            .entry(self.requests[index.index()].specifier.clone())
            .or_default()
            .push(index);
        index
    }

    /// Add an export, marking conflicting declarations as unknown.
    /// If the entry already exists with only function/static metadata (no prior
    /// resolution), the resolution is set without conflict detection.
    pub fn add_export(&mut self, name: impl Into<SmolStr>, export: ModuleExport) {
        if self.unknown_exports {
            return;
        }
        let name = name.into();
        match self.exports.get(&name) {
            None => {
                self.exports.insert(name, ExportEntry::new(export));
            }
            Some(existing)
                if existing.resolution.is_none() || existing.resolution == Some(export.clone()) =>
            {
                if let Some(entry) = self.exports.get_mut(&name) {
                    entry.resolution = Some(export);
                }
            }
            Some(_) => {
                if let Some(entry) = self.exports.get_mut(&name) {
                    entry.resolution = Some(ModuleExport::Unknown);
                    entry.function_id = None;
                    entry.static_value = None;
                }
            }
        }
    }

    pub(in crate::analysis) fn add_function_export(
        &mut self,
        name: impl Into<SmolStr>,
        function: FunctionId,
    ) {
        let name = name.into();
        match self.exports.get(&name) {
            None => {
                self.exports.insert(
                    name,
                    ExportEntry {
                        resolution: None,
                        function_id: Some(function),
                        static_value: None,
                    },
                );
            }
            Some(existing) if existing.function_id == Some(function) => {}
            Some(_) => {
                if let Some(entry) = self.exports.get_mut(&name) {
                    entry.function_id = None;
                }
            }
        }
    }

    /// Record a statically exported string value.
    pub fn add_static_string(&mut self, name: impl Into<SmolStr>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        match self.exports.get_mut(&name) {
            Some(entry) => {
                entry.static_value = Some(value);
            }
            None => {
                self.exports.insert(
                    name,
                    ExportEntry {
                        resolution: None,
                        function_id: None,
                        static_value: Some(value),
                    },
                );
            }
        }
    }

    /// Append a star-export request while the interface remains known.
    pub fn add_star_export(&mut self, request: ModuleRequestId) {
        if !self.unknown_exports {
            self.star_exports.push(request);
        }
    }

    /// Invalidate all export claims after a dynamic or ambiguous shape.
    pub fn mark_unknown_exports(&mut self) {
        self.exports.clear();
        self.star_exports.clear();
        self.unknown_exports = true;
    }

    /// Whether at least one known or deferred export exists.
    pub fn has_exports(&self) -> bool {
        self.exports.values().any(|e| e.resolution.is_some()) || !self.star_exports.is_empty()
    }

    /// Iterate authored requests in source/insertion order.
    pub fn requests(&self) -> impl Iterator<Item = &ModuleRequest> {
        self.requests.iter()
    }

    /// Borrow one request by its stable local index.
    pub fn request(&self, index: ModuleRequestId) -> Option<&ModuleRequest> {
        self.requests.get(index.index())
    }

    /// Return request IDs authored with one literal specifier in source order.
    pub(in crate::analysis) fn request_ids_for_specifier(
        &self,
        specifier: &str,
    ) -> impl Iterator<Item = ModuleRequestId> + '_ {
        self.requests_by_specifier
            .get(specifier)
            .into_iter()
            .flat_map(|requests| requests.iter().copied())
    }

    /// Iterate deferred star-export request indices.
    pub fn star_exports(&self) -> impl Iterator<Item = &ModuleRequestId> {
        self.star_exports.iter()
    }

    /// Iterate named exports in deterministic key order.
    pub fn exports(&self) -> impl Iterator<Item = (&SmolStr, &ModuleExport)> {
        self.exports
            .iter()
            .filter_map(|(k, v)| v.resolution.as_ref().map(|r| (k, r)))
    }

    /// Whether a local binding of this name was recorded.
    pub fn is_local(&self, name: &str) -> bool {
        self.locals.contains(name)
    }

    /// Whether the interface contains an unsupported export shape.
    pub fn is_unknown(&self) -> bool {
        self.unknown_exports
    }

    /// Return a statically exported string, if present.
    pub fn static_string(&self, name: &str) -> Option<&String> {
        self.exports.get(name).and_then(|e| e.static_value.as_ref())
    }

    pub(in crate::analysis) fn function_export(&self, name: &str) -> Option<FunctionId> {
        self.exports.get(name).and_then(|e| e.function_id)
    }

    /// Project authored requests into (ModuleRequestId, ResolutionRequest)
    /// pairs using the typed importer path and source line index.
    pub(crate) fn requests_with_ids(
        &self,
        importer: &ProjectRelativePath,
        lines: &crate::SourceLineIndex,
    ) -> Vec<(ModuleRequestId, ResolutionRequest)> {
        self.requests
            .iter()
            .filter_map(|request| {
                Some((
                    request.id(),
                    ResolutionRequest {
                        key: ResolutionRequestKey {
                            importer: importer.clone(),
                            kind: request.kind(),
                            range: lines.try_range(request.span()).ok()?,
                        },
                        request: request.specifier().to_string(),
                    },
                ))
            })
            .collect()
    }
}
