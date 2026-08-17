pub(super) mod classification;

pub(super) use classification::{DeclarationClassification, classify_declaration};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
