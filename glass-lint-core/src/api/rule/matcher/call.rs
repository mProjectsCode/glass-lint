use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallMatcher {
    pub name: String,
    pub provenance: CallProvenance,
    pub arg_strings: Vec<ArgStringMatcher>,
}

impl CallMatcher {
    pub fn heuristic(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provenance: CallProvenance::Any,
            arg_strings: Vec::new(),
        }
    }

    pub fn global(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provenance: CallProvenance::Global,
            arg_strings: Vec::new(),
        }
    }

    pub fn module_export(module: impl Into<String>, export: impl Into<String>) -> Self {
        Self {
            name: export.into(),
            provenance: CallProvenance::ModuleExport {
                module: module.into(),
            },
            arg_strings: Vec::new(),
        }
    }

    pub fn static_string_arg(mut self, index: usize) -> Self {
        self.arg_strings.push(ArgStringMatcher {
            index,
            values: Vec::new(),
            predicate: None,
        });
        self
    }

    pub fn arg_string<I, S>(mut self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arg_strings.push(ArgStringMatcher {
            index,
            values: values.into_iter().map(Into::into).collect(),
            predicate: None,
        });
        self
    }

    pub fn arg_value(mut self, index: usize, value: FlowValueMatcher) -> Self {
        self.arg_strings.push(ArgStringMatcher {
            index,
            values: Vec::new(),
            predicate: Some(value),
        });
        self
    }

    pub fn evidence_symbol(&self) -> String {
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
