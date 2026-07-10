mod eval;
mod header_indicator;
mod private_address;
mod remote_resource;
mod request;
mod script_injection;
mod service_indicator;
mod string_timer;
mod telemetry_indicator;
mod url_construction;

use glass_lint_core::rules::Rule;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        request::rule(),
        url_construction::rule(),
        private_address::rule(),
        service_indicator::rule(),
        telemetry_indicator::rule(),
        header_indicator::rule(),
        remote_resource::rule(),
        eval::rule(),
        string_timer::rule(),
        script_injection::rule(),
    ]
}
