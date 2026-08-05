use std::collections::{BTreeMap, BTreeSet};

use glass_lint_datastructures::ByteRange;
use smol_str::SmolStr;

use crate::{
    analysis::model::scope::FunctionId,
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
pub enum ModuleRequestRole {
    Import { bindings: Vec<ImportedBinding> },
    ReExport { bindings: Vec<ReExportBinding> },
    StarExport,
    DynamicImport,
    Require,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportedBinding {
    imported: Option<SmolStr>,
    local: SmolStr,
    namespace: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReExportBinding {
    imported: SmolStr,
    exported: SmolStr,
    namespace: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleRequest {
    id: ModuleRequestId,
    span: ByteRange,
    kind: ResolutionRequestKind,
    specifier: SmolStr,
    role: ModuleRequestRole,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModuleExport {
    Local {
        name: SmolStr,
    },
    Value,
    ReExport {
        request: ModuleRequestId,
        imported: SmolStr,
    },
    Namespace {
        request: ModuleRequestId,
    },
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExportEntry {
    resolution: Option<ModuleExport>,
    function_id: Option<FunctionId>,
    static_value: Option<String>,
}

impl ExportEntry {
    fn with_resolution(resolution: ModuleExport) -> Self {
        Self {
            resolution: Some(resolution),
            function_id: None,
            static_value: None,
        }
    }

    fn with_function(function: FunctionId) -> Self {
        Self {
            resolution: None,
            function_id: Some(function),
            static_value: None,
        }
    }

    fn with_static_string(value: String) -> Self {
        Self {
            resolution: None,
            function_id: None,
            static_value: Some(value),
        }
    }

    fn set_resolution(&mut self, resolution: ModuleExport) {
        self.resolution = Some(resolution);
    }

    fn clear_function(&mut self) {
        self.function_id = None;
    }

    fn set_static_string(&mut self, value: String) {
        self.static_value = Some(value);
    }

    fn mark_unknown(&mut self) {
        self.resolution = Some(ModuleExport::Unknown);
        self.function_id = None;
        self.static_value = None;
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ModuleInterface {
    requests: Vec<ModuleRequest>,
    requests_by_specifier: BTreeMap<SmolStr, Vec<ModuleRequestId>>,
    exports: BTreeMap<SmolStr, ExportEntry>,
    star_exports: Vec<ModuleRequestId>,
    locals: BTreeSet<SmolStr>,
    unknown_exports: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ModuleRequestId(usize);

impl ModuleRequestId {
    fn index(self) -> usize {
        self.0
    }
}

impl ImportedBinding {
    pub fn new(imported: Option<SmolStr>, local: SmolStr, namespace: bool) -> Self {
        Self {
            imported,
            local,
            namespace,
        }
    }

    pub fn imported(&self) -> Option<&SmolStr> {
        self.imported.as_ref()
    }

    pub fn is_namespace(&self) -> bool {
        self.namespace
    }
}

impl ReExportBinding {
    pub fn new(imported: SmolStr, exported: SmolStr, namespace: bool) -> Self {
        Self {
            imported,
            exported,
            namespace,
        }
    }
}

impl ModuleRequest {
    pub fn id(&self) -> ModuleRequestId {
        self.id
    }

    pub fn span(&self) -> ByteRange {
        self.span
    }

    pub fn kind(&self) -> ResolutionRequestKind {
        self.kind
    }

    pub fn specifier(&self) -> &SmolStr {
        &self.specifier
    }

    pub fn role(&self) -> &ModuleRequestRole {
        &self.role
    }
}

impl ModuleInterface {
    pub fn add_local(&mut self, name: impl Into<SmolStr>) {
        self.locals.insert(name.into());
    }

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

    pub fn add_export(&mut self, name: impl Into<SmolStr>, export: ModuleExport) {
        if self.unknown_exports {
            return;
        }
        let name = name.into();
        match self.exports.get(&name) {
            None => {
                self.exports
                    .insert(name, ExportEntry::with_resolution(export));
            }
            Some(existing)
                if existing.resolution.is_none() || existing.resolution == Some(export.clone()) =>
            {
                if let Some(entry) = self.exports.get_mut(&name) {
                    entry.set_resolution(export);
                }
            }
            Some(_) => {
                if let Some(entry) = self.exports.get_mut(&name) {
                    entry.mark_unknown();
                }
            }
        }
    }

    pub fn add_function_export(&mut self, name: impl Into<SmolStr>, function: FunctionId) {
        if self.unknown_exports {
            return;
        }
        let name = name.into();
        match self.exports.get(&name) {
            None => {
                self.exports
                    .insert(name, ExportEntry::with_function(function));
            }
            Some(existing) if existing.function_id == Some(function) => {}
            Some(_) => {
                if let Some(entry) = self.exports.get_mut(&name) {
                    entry.clear_function();
                }
            }
        }
    }

    pub fn add_static_string(&mut self, name: impl Into<SmolStr>, value: impl Into<String>) {
        if self.unknown_exports {
            return;
        }
        let name = name.into();
        let value = value.into();
        match self.exports.get_mut(&name) {
            Some(entry) => {
                entry.set_static_string(value);
            }
            None => {
                self.exports
                    .insert(name, ExportEntry::with_static_string(value));
            }
        }
    }

    pub fn add_star_export(&mut self, request: ModuleRequestId) {
        if !self.unknown_exports {
            self.star_exports.push(request);
        }
    }

    pub fn mark_unknown_exports(&mut self) {
        self.exports.clear();
        self.star_exports.clear();
        self.unknown_exports = true;
    }

    pub fn has_exports(&self) -> bool {
        self.exports.values().any(|e| e.resolution.is_some()) || !self.star_exports.is_empty()
    }

    pub fn requests(&self) -> impl Iterator<Item = &ModuleRequest> {
        self.requests.iter()
    }

    pub fn request(&self, index: ModuleRequestId) -> Option<&ModuleRequest> {
        self.requests.get(index.index())
    }

    pub fn request_ids_for_specifier(
        &self,
        specifier: &str,
    ) -> impl Iterator<Item = ModuleRequestId> + '_ {
        self.requests_by_specifier
            .get(specifier)
            .into_iter()
            .flat_map(|requests| requests.iter().copied())
    }

    pub fn star_exports(&self) -> impl Iterator<Item = &ModuleRequestId> {
        self.star_exports.iter()
    }

    pub fn exports(&self) -> impl Iterator<Item = (&SmolStr, &ModuleExport)> {
        self.exports
            .iter()
            .filter_map(|(k, v)| v.resolution.as_ref().map(|r| (k, r)))
    }

    pub fn is_local(&self, name: &str) -> bool {
        self.locals.contains(name)
    }

    pub fn is_unknown(&self) -> bool {
        self.unknown_exports
    }

    pub fn static_string(&self, name: &str) -> Option<&str> {
        self.exports
            .get(name)
            .and_then(|e| e.static_value.as_deref())
    }

    pub fn function_export(&self, name: &str) -> Option<FunctionId> {
        self.exports.get(name).and_then(|e| e.function_id)
    }

    pub fn requests_with_ids(
        &self,
        importer: &ProjectRelativePath,
        lines: &crate::SourceLineIndex,
    ) -> Vec<(ModuleRequestId, ResolutionRequest)> {
        self.requests
            .iter()
            .filter_map(|request| {
                Some((
                    request.id(),
                    ResolutionRequest::new(
                        ResolutionRequestKey::new(
                            importer.clone(),
                            request.kind(),
                            lines.try_range(request.span()).ok()?,
                        ),
                        request.specifier().clone(),
                    ),
                ))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_exports_reject_function_and_static_metadata() {
        let mut interface = ModuleInterface::default();
        interface.add_function_export("function", FunctionId::from_test(1));
        interface.add_static_string("text", "before");

        interface.mark_unknown_exports();
        interface.add_function_export("late-function", FunctionId::from_test(2));
        interface.add_static_string("late-text", "after");

        assert!(interface.is_unknown());
        assert_eq!(interface.exports().count(), 0);
        assert_eq!(interface.function_export("function"), None);
        assert_eq!(interface.static_string("text"), None);
        assert_eq!(interface.function_export("late-function"), None);
        assert_eq!(interface.static_string("late-text"), None);
    }
}
