use std::collections::BTreeSet;

use smol_str::SmolStr;
use swc_ecma_ast::ImportDecl;

use crate::analysis::{
    model::module::{ImportedBinding, ModuleRequestId},
    module_request::{ModuleRequestKind, RecognizedModuleRequest},
    syntax::collect_pat_bindings,
};

mod commonjs;
mod exports;

pub(super) struct ModuleInterfaceBuilder {
    interface: crate::analysis::model::module::ModuleInterface,
}

impl ModuleInterfaceBuilder {
    pub(in crate::analysis::facts) fn new() -> Self {
        Self {
            interface: crate::analysis::model::module::ModuleInterface::default(),
        }
    }

    pub(in crate::analysis::facts) fn finish(
        self,
    ) -> crate::analysis::model::module::ModuleInterface {
        self.interface
    }

    pub(in crate::analysis::facts) fn record_local(&mut self, name: impl Into<SmolStr>) {
        self.interface.add_local(name);
    }

    pub(in crate::analysis::facts) fn record_pattern_locals(
        &mut self,
        pattern: &swc_ecma_ast::Pat,
    ) {
        for name in Self::collect_pattern_locals(pattern) {
            self.interface.add_local(name);
        }
    }

    pub(in crate::analysis::facts) fn collect_pattern_locals(
        pattern: &swc_ecma_ast::Pat,
    ) -> BTreeSet<SmolStr> {
        let mut names = BTreeSet::new();
        collect_pat_bindings(pattern, &mut names);
        names
    }

    pub(in crate::analysis::facts) fn add_import_request(
        &mut self,
        span: glass_lint_datastructures::ByteRange,
        specifier: impl Into<SmolStr>,
        bindings: Vec<ImportedBinding>,
    ) -> ModuleRequestId {
        self.interface.add_import_request(span, specifier, bindings)
    }

    pub(in crate::analysis::facts) fn add_reexport_request(
        &mut self,
        span: glass_lint_datastructures::ByteRange,
        specifier: impl Into<SmolStr>,
    ) -> ModuleRequestId {
        self.interface.add_reexport_request(span, specifier)
    }

    pub(in crate::analysis::facts) fn mark_unknown_exports(&mut self) {
        self.interface.mark_unknown_exports();
    }

    fn has_exports(&self) -> bool {
        self.interface.has_exports()
    }

    fn add_export(
        &mut self,
        name: impl Into<SmolStr>,
        export: crate::analysis::model::module::ModuleExport,
    ) {
        self.interface.add_export(name, export);
    }

    fn add_function_export(
        &mut self,
        name: impl Into<SmolStr>,
        function: crate::analysis::model::scope::FunctionId,
    ) {
        self.interface.add_function_export(name, function);
    }

    fn add_static_string(&mut self, name: impl Into<SmolStr>, value: impl Into<String>) {
        self.interface.add_static_string(name, value);
    }

    fn add_star_export_request(
        &mut self,
        span: glass_lint_datastructures::ByteRange,
        specifier: impl Into<smol_str::SmolStr>,
    ) -> ModuleRequestId {
        self.interface.add_star_export_request(span, specifier)
    }

    pub(in crate::analysis::facts) fn record_local_imports(&mut self, import: &ImportDecl) {
        for specifier in &import.specifiers {
            if !specifier.is_type_only() {
                self.record_local(specifier.local().sym.to_string());
            }
        }
    }

    pub(in crate::analysis::facts) fn record_module_request(
        &mut self,
        span: glass_lint_datastructures::ByteRange,
        request: &RecognizedModuleRequest,
    ) -> Option<String> {
        let module = request.module().to_owned();
        match request.kind() {
            ModuleRequestKind::DynamicImport => {
                self.interface.add_dynamic_import_request(span, &module);
            }
            ModuleRequestKind::Require => {
                self.interface.add_require_request(span, &module);
            }
            ModuleRequestKind::WrappedRequire => return None,
        }
        Some(module)
    }
}
