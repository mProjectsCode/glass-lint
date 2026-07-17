//! General JavaScript and network-indicator rule catalog.
//!
//! These rules combine strict global/module provenance with explicitly marked
//! literal heuristics; the catalog preserves their deterministic provider
//! order.

mod eval;
mod header_indicator;
mod private_address;
mod service_indicator;
mod string_timer;
mod telemetry_indicator;
mod url_construction;

use glass_lint_core::rules::Rule;

pub fn rules() -> Vec<Rule> {
    // Keep executable-code rules first, then network indicators in a stable
    // order so catalog metadata and findings do not depend on module discovery.
    vec![
        eval::rule(),
        url_construction::rule(),
        private_address::rule(),
        service_indicator::rule(),
        telemetry_indicator::rule(),
        header_indicator::rule(),
        string_timer::rule(),
    ]
}
