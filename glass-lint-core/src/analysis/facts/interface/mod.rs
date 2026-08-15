use std::collections::BTreeSet;

use smol_str::SmolStr;
use swc_ecma_ast::ImportDecl;

use crate::analysis::{
    model::module::ModuleInterface,
    module_request::{ModuleRequestKind, RecognizedModuleRequest},
    syntax::collect_pat_bindings,
};

mod commonjs;
mod exports;

impl ModuleInterface {
    pub(in crate::analysis::facts) fn record_pattern_locals(
        &mut self,
        pattern: &swc_ecma_ast::Pat,
    ) -> BTreeSet<SmolStr> {
        let mut names = BTreeSet::new();
        collect_pat_bindings(pattern, &mut names);
        for name in &names {
            self.add_local(name.clone());
        }
        names
    }

    pub(in crate::analysis::facts) fn record_local_imports(&mut self, import: &ImportDecl) {
        for specifier in &import.specifiers {
            if !specifier.is_type_only() {
                self.add_local(specifier.local().sym.to_string());
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
                self.add_dynamic_import_request(span, &module);
            }
            ModuleRequestKind::Require => {
                self.add_require_request(span, &module);
            }
            ModuleRequestKind::WrappedRequire => return None,
        }
        Some(module)
    }
}
