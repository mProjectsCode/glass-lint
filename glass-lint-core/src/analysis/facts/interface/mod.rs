use std::collections::BTreeSet;

use smol_str::SmolStr;
use swc_ecma_ast::ImportDecl;

use crate::{
    analysis::{
        module::{ModuleRequestId, ModuleRequestRole},
        syntax::collect_pat_bindings,
    },
    project::ResolutionRequestKind,
};

mod commonjs;
mod exports;

pub(super) struct ModuleInterfaceBuilder {
    interface: crate::analysis::module::ModuleInterface,
}

impl ModuleInterfaceBuilder {
    pub(in crate::analysis::facts) fn new() -> Self {
        Self {
            interface: crate::analysis::module::ModuleInterface::default(),
        }
    }

    pub(in crate::analysis::facts) fn finish(self) -> crate::analysis::module::ModuleInterface {
        self.interface
    }

    pub(in crate::analysis::facts) fn record_local(&mut self, name: impl Into<SmolStr>) {
        self.interface.add_local(name);
    }

    pub(in crate::analysis::facts) fn record_pattern_locals(
        &mut self,
        pattern: &swc_ecma_ast::Pat,
    ) -> BTreeSet<SmolStr> {
        let mut names = BTreeSet::new();
        collect_pat_bindings(pattern, &mut names);
        for name in &names {
            self.interface.add_local(name.clone());
        }
        names
    }

    pub(in crate::analysis::facts) fn add_request(
        &mut self,
        span: glass_lint_datastructures::ByteRange,
        kind: ResolutionRequestKind,
        specifier: impl Into<SmolStr>,
        role: ModuleRequestRole,
    ) -> ModuleRequestId {
        self.interface.add_request(span, kind, specifier, role)
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
        export: crate::analysis::module::ModuleExport,
    ) {
        self.interface.add_export(name, export);
    }

    fn add_function_export(
        &mut self,
        name: impl Into<SmolStr>,
        function: crate::analysis::value::FunctionId,
    ) {
        self.interface.add_function_export(name, function);
    }

    fn add_static_string(&mut self, name: impl Into<SmolStr>, value: impl Into<String>) {
        self.interface.add_static_string(name, value);
    }

    fn add_star_export(&mut self, request: ModuleRequestId) {
        self.interface.add_star_export(request);
    }

    pub(in crate::analysis::facts) fn record_local_imports(&mut self, import: &ImportDecl) {
        for specifier in &import.specifiers {
            if !specifier.is_type_only() {
                self.record_local(specifier.local().sym.to_string());
            }
        }
    }

    pub(in crate::analysis::facts) fn record_import_request(
        &mut self,
        span: glass_lint_datastructures::ByteRange,
        specifier: &str,
    ) {
        self.interface.add_request(
            span,
            ResolutionRequestKind::DynamicImport,
            specifier,
            ModuleRequestRole::DynamicImport,
        );
    }

    pub(in crate::analysis::facts) fn record_require_request(
        &mut self,
        span: glass_lint_datastructures::ByteRange,
        specifier: &str,
    ) {
        self.interface.add_request(
            span,
            ResolutionRequestKind::Require,
            specifier,
            ModuleRequestRole::Require,
        );
    }
}
