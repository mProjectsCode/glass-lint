use super::{ArgumentConstraint, ArgumentMatcher, ValueMatcher};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallMatcher {
    pub(crate) name: String,
    pub(crate) provenance: CallProvenance,
    pub(crate) arguments: Vec<ArgumentConstraint>,
}

impl CallMatcher {
    pub fn heuristic(name: impl Into<String>) -> Self {
        Self::new(name, CallProvenance::Any)
    }

    pub fn global(name: impl Into<String>) -> Self {
        Self::new(name, CallProvenance::Global)
    }

    pub fn module_export(module: impl Into<String>, export: impl Into<String>) -> Self {
        Self::new(
            export,
            CallProvenance::ModuleExport {
                module: module.into(),
            },
        )
    }

    fn new(name: impl Into<String>, provenance: CallProvenance) -> Self {
        Self {
            name: name.into(),
            provenance,
            arguments: Vec::new(),
        }
    }

    pub fn arg(mut self, index: usize, matcher: impl Into<ArgumentMatcher>) -> Self {
        self.arguments.push(ArgumentConstraint {
            index,
            matcher: matcher.into(),
        });
        self
    }

    #[must_use]
    pub fn static_string_arg(self, index: usize) -> Self {
        self.arg(index, ValueMatcher::static_string())
    }

    pub fn arg_string<I, S>(self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arg(index, ValueMatcher::static_string().equals_any(values))
    }

    pub fn arg_value(self, index: usize, value: impl Into<ValueMatcher>) -> Self {
        self.arg(index, value.into())
    }

    pub(crate) fn evidence_symbol(&self) -> String {
        match &self.provenance {
            CallProvenance::Any | CallProvenance::Global => self.name.clone(),
            CallProvenance::ModuleExport { module } => format!("{module}.{}", self.name),
        }
    }

    pub(crate) fn sort_key(&self) -> (&str, &str) {
        match &self.provenance {
            CallProvenance::Any => ("any", &self.name),
            CallProvenance::Global => ("global", &self.name),
            CallProvenance::ModuleExport { module } => (module, &self.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallProvenance {
    Any,
    Global,
    ModuleExport { module: String },
}

#[allow(dead_code)]
fn _value_matcher_is_used(_: ValueMatcher) {}
