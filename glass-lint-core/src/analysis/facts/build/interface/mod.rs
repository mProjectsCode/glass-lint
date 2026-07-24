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
    pub(in crate::analysis::facts::build) fn new() -> Self {
        Self {
            interface: crate::analysis::module::ModuleInterface::default(),
        }
    }

    pub(in crate::analysis::facts::build) fn finish(self) -> crate::analysis::module::ModuleInterface {
        self.interface
    }

    pub(in crate::analysis::facts::build) fn record_local(&mut self, name: impl Into<SmolStr>) {
        self.interface.add_local(name);
    }

    pub(in crate::analysis::facts::build) fn record_pattern_locals(&mut self, pattern: &swc_ecma_ast::Pat) {
        let mut names = std::collections::BTreeSet::new();
        collect_pat_bindings(pattern, &mut names);
        for name in names {
            self.interface.add_local(name);
        }
    }

    pub(in crate::analysis::facts::build) fn add_request(
        &mut self,
        span: glass_lint_datastructures::ByteRange,
        kind: ResolutionRequestKind,
        specifier: impl Into<SmolStr>,
        role: ModuleRequestRole,
    ) -> ModuleRequestId {
        self.interface.add_request(span, kind, specifier, role)
    }

    pub(in crate::analysis::facts::build) fn mark_unknown_exports(&mut self) {
        self.interface.mark_unknown_exports();
    }

    pub(in crate::analysis::facts::build) fn record_local_imports(&mut self, import: &ImportDecl) {
        for specifier in &import.specifiers {
            if !specifier.is_type_only() {
                self.record_local(specifier.local().sym.to_string());
            }
        }
    }

    pub(in crate::analysis::facts::build) fn record_import_request(
        &mut self,
        span: glass_lint_datastructures::ByteRange,
        specifier: &swc_ecma_ast::Str,
    ) {
        self.interface.add_request(
            span,
            ResolutionRequestKind::DynamicImport,
            specifier.value.to_string_lossy(),
            ModuleRequestRole::DynamicImport,
        );
    }

    pub(in crate::analysis::facts::build) fn record_require_request(
        &mut self,
        span: glass_lint_datastructures::ByteRange,
        specifier: &swc_ecma_ast::Str,
    ) {
        self.interface.add_request(
            span,
            ResolutionRequestKind::Require,
            specifier.value.to_string_lossy(),
            ModuleRequestRole::Require,
        );
    }
}
