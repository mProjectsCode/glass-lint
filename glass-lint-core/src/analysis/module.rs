//! Public-matcher-independent module requests and export interfaces.
//!
//! Types have been moved to [`crate::analysis::model::module`].

pub use crate::analysis::model::module::{
    COMMONJS_EXPORTS, COMMONJS_MODULE, COMMONJS_REQUIRE, DEFAULT_EXPORT, ImportedBinding,
    ModuleExport, ModuleInterface, ModuleRequest, ModuleRequestId, ModuleRequestRole,
    NAMESPACE_EXPORT, ReExportBinding,
};
