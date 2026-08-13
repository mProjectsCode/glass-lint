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
pub struct ImportedBinding {
    imported: Option<SmolStr>,
    namespace: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleRequest {
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ExportObservation {
    resolution: Option<ModuleExport>,
    function_id: Option<FunctionId>,
    static_value: Option<String>,
}

impl ExportObservation {
    fn resolution(resolution: ModuleExport) -> Self {
        Self {
            resolution: Some(resolution),
            ..Self::default()
        }
    }

    fn function(function_id: FunctionId) -> Self {
        Self {
            function_id: Some(function_id),
            ..Self::default()
        }
    }

    fn static_string(static_value: String) -> Self {
        Self {
            static_value: Some(static_value),
            ..Self::default()
        }
    }
}

enum ExportMerge {
    Unchanged,
    Updated,
    Contradiction,
}

impl ExportEntry {
    fn from_observation(observation: ExportObservation) -> Self {
        Self {
            resolution: observation.resolution,
            function_id: observation.function_id,
            static_value: observation.static_value,
        }
    }

    fn mark_unknown(&mut self) {
        self.resolution = Some(ModuleExport::Unknown);
        self.function_id = None;
        self.static_value = None;
    }

    fn merge(&mut self, observation: ExportObservation) -> ExportMerge {
        if self.resolution == Some(ModuleExport::Unknown) {
            return ExportMerge::Unchanged;
        }
        if matches!(observation.resolution, Some(ModuleExport::Unknown))
            || Self::conflicts(self.resolution.as_ref(), observation.resolution.as_ref())
            || Self::conflicts(self.function_id.as_ref(), observation.function_id.as_ref())
            || Self::conflicts(
                self.static_value.as_ref(),
                observation.static_value.as_ref(),
            )
        {
            self.mark_unknown();
            return ExportMerge::Contradiction;
        }

        let resolution_updated = self.resolution.is_none() && observation.resolution.is_some();
        if resolution_updated {
            self.resolution = observation.resolution;
        }
        let function_updated = self.function_id.is_none() && observation.function_id.is_some();
        if function_updated {
            self.function_id = observation.function_id;
        }
        let static_updated = self.static_value.is_none() && observation.static_value.is_some();
        if static_updated {
            self.static_value = observation.static_value;
        }
        if resolution_updated || function_updated || static_updated {
            ExportMerge::Updated
        } else {
            ExportMerge::Unchanged
        }
    }

    fn conflicts<T: PartialEq>(current: Option<&T>, observation: Option<&T>) -> bool {
        matches!((current, observation), (Some(current), Some(observation)) if current != observation)
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
    pub fn new(imported: Option<SmolStr>, namespace: bool) -> Self {
        Self {
            imported,
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

impl ModuleRequest {
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

    fn add_request(
        &mut self,
        span: ByteRange,
        kind: ResolutionRequestKind,
        specifier: impl Into<SmolStr>,
        role: ModuleRequestRole,
    ) -> ModuleRequestId {
        let index = ModuleRequestId(self.requests.len());
        self.requests.push(ModuleRequest {
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

    pub fn add_import_request(
        &mut self,
        span: ByteRange,
        specifier: impl Into<SmolStr>,
        bindings: Vec<ImportedBinding>,
    ) -> ModuleRequestId {
        self.add_request(
            span,
            ResolutionRequestKind::StaticImport,
            specifier,
            ModuleRequestRole::Import { bindings },
        )
    }

    pub fn add_reexport_request(
        &mut self,
        span: ByteRange,
        specifier: impl Into<SmolStr>,
    ) -> ModuleRequestId {
        self.add_request(
            span,
            ResolutionRequestKind::StaticImport,
            specifier,
            ModuleRequestRole::ReExport,
        )
    }

    pub fn add_dynamic_import_request(
        &mut self,
        span: ByteRange,
        specifier: impl Into<SmolStr>,
    ) -> ModuleRequestId {
        self.add_request(
            span,
            ResolutionRequestKind::DynamicImport,
            specifier,
            ModuleRequestRole::DynamicImport,
        )
    }

    pub fn add_require_request(
        &mut self,
        span: ByteRange,
        specifier: impl Into<SmolStr>,
    ) -> ModuleRequestId {
        self.add_request(
            span,
            ResolutionRequestKind::Require,
            specifier,
            ModuleRequestRole::Require,
        )
    }

    pub fn add_export(&mut self, name: impl Into<SmolStr>, export: ModuleExport) {
        self.observe_export(name.into(), ExportObservation::resolution(export));
    }

    pub fn add_function_export(&mut self, name: impl Into<SmolStr>, function: FunctionId) {
        self.observe_export(name.into(), ExportObservation::function(function));
    }

    pub fn add_static_string(&mut self, name: impl Into<SmolStr>, value: impl Into<String>) {
        self.observe_export(name.into(), ExportObservation::static_string(value.into()));
    }

    fn observe_export(&mut self, name: SmolStr, observation: ExportObservation) {
        if self.unknown_exports {
            return;
        }
        match self.exports.entry(name) {
            Entry::Vacant(entry) => {
                entry.insert(ExportEntry::from_observation(observation));
            }
            Entry::Occupied(mut entry) => {
                let _ = entry.get_mut().merge(observation);
            }
        }
    }

    pub fn add_star_export_request(
        &mut self,
        span: ByteRange,
        specifier: impl Into<SmolStr>,
    ) -> ModuleRequestId {
        if self.unknown_exports {
            self.add_request(
                span,
                ResolutionRequestKind::StaticImport,
                specifier,
                ModuleRequestRole::StarExport,
            )
        } else {
            let request = self.add_request(
                span,
                ResolutionRequestKind::StaticImport,
                specifier,
                ModuleRequestRole::StarExport,
            );
            self.star_exports.push(request);
            request
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

    pub fn requests_with_ids(
        &self,
        importer: &ProjectRelativePath,
        lines: &crate::SourceLineIndex,
    ) -> Vec<(ModuleRequestId, ResolutionRequest)> {
        self.requests
            .iter()
            .enumerate()
            .filter_map(|(index, request)| {
                Some((
                    ModuleRequestId(index),
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

    #[test]
    fn compatible_export_observations_merge_independently_of_order() {
        let function = FunctionId::from_test(1);
        let resolution = ModuleExport::Local {
            name: "value".into(),
        };

        let mut first = ModuleInterface::default();
        first.add_function_export("value", function);
        first.add_static_string("value", "text");
        first.add_export("value", resolution.clone());

        let mut second = ModuleInterface::default();
        second.add_export("value", resolution);
        second.add_static_string("value", "text");
        second.add_function_export("value", function);

        assert_eq!(first, second);
        assert_eq!(first.function_export("value"), Some(function));
        assert_eq!(first.static_string("value"), Some("text"));
    }

    #[test]
    fn conflicting_export_observations_clear_all_metadata() {
        let mut interface = ModuleInterface::default();
        interface.add_export("value", ModuleExport::Value);
        interface.add_function_export("value", FunctionId::from_test(1));
        interface.add_static_string("value", "text");
        interface.add_function_export("value", FunctionId::from_test(2));

        let Some((name, export)) = interface.exports().next() else {
            panic!("conflict should retain an unknown export entry");
        };
        assert_eq!(name.as_str(), "value");
        assert_eq!(export, &ModuleExport::Unknown);
        assert_eq!(interface.function_export("value"), None);
        assert_eq!(interface.static_string("value"), None);
    }

    #[test]
    fn conflicting_static_strings_clear_the_export_entry() {
        let mut interface = ModuleInterface::default();
        interface.add_static_string("value", "first");
        interface.add_static_string("value", "second");

        assert_eq!(interface.static_string("value"), None);
        let Some((name, export)) = interface.exports().next() else {
            panic!("conflict should retain an unknown export entry");
        };
        assert_eq!(name.as_str(), "value");
        assert_eq!(export, &ModuleExport::Unknown);
    }

    #[test]
    fn request_constructors_retain_their_valid_kind_and_role_pair() {
        let span = ByteRange::new(0, 1).unwrap();
        let mut interface = ModuleInterface::default();
        interface.add_import_request(
            span,
            "imported",
            vec![ImportedBinding::new(Some("default".into()), false)],
        );
        interface.add_reexport_request(span, "reexported");
        interface.add_star_export_request(span, "starred");
        interface.add_dynamic_import_request(span, "dynamic");
        interface.add_require_request(span, "required");

        let requests = interface.requests().collect::<Vec<_>>();
        assert_eq!(requests.len(), 5);
        assert_eq!(requests[0].kind(), ResolutionRequestKind::StaticImport);
        assert!(matches!(
            requests[0].role(),
            ModuleRequestRole::Import { .. }
        ));
        assert_eq!(requests[1].kind(), ResolutionRequestKind::StaticImport);
        assert_eq!(requests[1].role(), &ModuleRequestRole::ReExport);
        assert_eq!(requests[2].kind(), ResolutionRequestKind::StaticImport);
        assert_eq!(requests[2].role(), &ModuleRequestRole::StarExport);
        assert_eq!(requests[3].kind(), ResolutionRequestKind::DynamicImport);
        assert_eq!(requests[3].role(), &ModuleRequestRole::DynamicImport);
        assert_eq!(requests[4].kind(), ResolutionRequestKind::Require);
        assert_eq!(requests[4].role(), &ModuleRequestRole::Require);
    }
}
