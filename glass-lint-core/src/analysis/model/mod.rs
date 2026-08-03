pub mod fact;
pub mod flow;
pub mod module;
pub mod scope;
pub mod value;

mod static_properties;

pub(in crate::analysis) use static_properties::StaticProperties;
