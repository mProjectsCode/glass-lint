mod code_block_processor;
mod link;
mod postprocessor;
mod render;
use glass_lint_core::rules::Rule;
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        postprocessor::rule(),
        code_block_processor::rule(),
        render::rule(),
        link::rule(),
    ]
}
