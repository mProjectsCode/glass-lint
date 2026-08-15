use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

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
    ReExport,
    StarExport,
    DynamicImport,
    Require,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImportedBinding {
    Named(SmolStr),
    Namespace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleRequest {
    span: ByteRange,
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
    fn mark_unknown(&mut self) {
        self.resolution = Some(ModuleExport::Unknown);
        self.function_id = None;
        self.static_value = None;
    }

    fn observe(
        &mut self,
        resolution: Option<ModuleExport>,
        function_id: Option<FunctionId>,
        static_value: Option<String>,
    ) {
        if self.resolution == Some(ModuleExport::Unknown) {
            return;
        }
        if matches!(resolution, Some(ModuleExport::Unknown))
            || Self::conflicts(self.resolution.as_ref(), resolution.as_ref())
            || Self::conflicts(self.function_id.as_ref(), function_id.as_ref())
            || Self::conflicts(self.static_value.as_ref(), static_value.as_ref())
        {
            self.mark_unknown();
            return;
        }
        if self.resolution.is_none() {
            self.resolution = resolution;
        }
        if self.function_id.is_none() {
            self.function_id = function_id;
        }
        if self.static_value.is_none() {
            self.static_value = static_value;
        }
    }

    fn conflicts<T: PartialEq>(current: Option<&T>, observed: Option<&T>) -> bool {
        matches!((current, observed), (Some(current), Some(observed)) if current != observed)
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
    pub fn named(imported: impl Into<SmolStr>) -> Self {
        Self::Named(imported.into())
    }

    pub const fn namespace() -> Self {
        Self::Namespace
    }

    pub fn is_namespace(&self) -> bool {
        matches!(self, Self::Namespace)
    }

    pub fn imported(&self) -> Option<&SmolStr> {
        match self {
            Self::Named(imported) => Some(imported),
            Self::Namespace => None,
        }
    }
}

impl ModuleRequest {
    pub fn span(&self) -> ByteRange {
        self.span
    }

    pub fn kind(&self) -> ResolutionRequestKind {
        match &self.role {
            ModuleRequestRole::Import { .. }
            | ModuleRequestRole::ReExport
            | ModuleRequestRole::StarExport => ResolutionRequestKind::StaticImport,
            ModuleRequestRole::DynamicImport => ResolutionRequestKind::DynamicImport,
            ModuleRequestRole::Require => ResolutionRequestKind::Require,
        }
    }

    pub fn specifier(&self) -> &SmolStr {
        &self.specifier
    }

    pub fn role(&self) -> &ModuleRequestRole {
        &self.role
    }
}

impl ModuleInterface {
    pub(in crate::analysis) fn add_local(&mut self, name: impl Into<SmolStr>) {
        self.locals.insert(name.into());
    }

    fn add_request(
        &mut self,
        span: ByteRange,
        specifier: impl Into<SmolStr>,
        role: ModuleRequestRole,
    ) -> ModuleRequestId {
        let index = ModuleRequestId(self.requests.len());
        self.requests.push(ModuleRequest {
            span,
            specifier: specifier.into(),
            role,
        });
        self.requests_by_specifier
            .entry(self.requests[index.index()].specifier.clone())
            .or_default()
            .push(index);
        index
    }

    pub(in crate::analysis) fn add_import_request(
        &mut self,
        span: ByteRange,
        specifier: impl Into<SmolStr>,
        bindings: Vec<ImportedBinding>,
    ) -> ModuleRequestId {
        self.add_request(span, specifier, ModuleRequestRole::Import { bindings })
    }

    pub(in crate::analysis) fn add_reexport_request(
        &mut self,
        span: ByteRange,
        specifier: impl Into<SmolStr>,
    ) -> ModuleRequestId {
        self.add_request(span, specifier, ModuleRequestRole::ReExport)
    }

    pub(in crate::analysis) fn add_dynamic_import_request(
        &mut self,
        span: ByteRange,
        specifier: impl Into<SmolStr>,
    ) -> ModuleRequestId {
        self.add_request(span, specifier, ModuleRequestRole::DynamicImport)
    }

    pub(in crate::analysis) fn add_require_request(
        &mut self,
        span: ByteRange,
        specifier: impl Into<SmolStr>,
    ) -> ModuleRequestId {
        self.add_request(span, specifier, ModuleRequestRole::Require)
    }

    pub(in crate::analysis) fn add_export(
        &mut self,
        name: impl Into<SmolStr>,
        export: ModuleExport,
    ) {
        self.observe_export(name.into(), Some(export), None, None);
    }

    pub(in crate::analysis) fn add_function_export(
        &mut self,
        name: impl Into<SmolStr>,
        function: FunctionId,
    ) {
        self.observe_export(name.into(), None, Some(function), None);
    }

    pub(in crate::analysis) fn add_static_string(
        &mut self,
        name: impl Into<SmolStr>,
        value: impl Into<String>,
    ) {
        self.observe_export(name.into(), None, None, Some(value.into()));
    }

    fn observe_export(
        &mut self,
        name: SmolStr,
        resolution: Option<ModuleExport>,
        function_id: Option<FunctionId>,
        static_value: Option<String>,
    ) {
        if self.unknown_exports {
            return;
        }
        match self.exports.entry(name) {
            Entry::Vacant(entry) => {
                entry.insert(ExportEntry {
                    resolution,
                    function_id,
                    static_value,
                });
            }
            Entry::Occupied(mut entry) => {
                entry
                    .get_mut()
                    .observe(resolution, function_id, static_value);
            }
        }
    }

    pub(in crate::analysis) fn add_star_export_request(
        &mut self,
        span: ByteRange,
        specifier: impl Into<SmolStr>,
    ) -> ModuleRequestId {
        if self.unknown_exports {
            self.add_request(span, specifier, ModuleRequestRole::StarExport)
        } else {
            let request = self.add_request(span, specifier, ModuleRequestRole::StarExport);
            self.star_exports.push(request);
            request
        }
    }

    pub(in crate::analysis) fn mark_unknown_exports(&mut self) {
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

    pub fn request_entries(&self) -> impl Iterator<Item = (ModuleRequestId, &ModuleRequest)> {
        self.requests
            .iter()
            .enumerate()
            .map(|(index, request)| (ModuleRequestId(index), request))
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

    pub fn for_each_request(
        &self,
        importer: &ProjectRelativePath,
        lines: &crate::SourceLineIndex,
        mut visit: impl FnMut(ModuleRequestId, ResolutionRequest),
    ) {
        for (index, request) in self.requests.iter().enumerate() {
            let Some(range) = lines.try_range(request.span()).ok() else {
                continue;
            };
            visit(
                ModuleRequestId(index),
                ResolutionRequest::new(
                    ResolutionRequestKey::new(importer.clone(), request.kind(), range),
                    request.specifier().clone(),
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests;
