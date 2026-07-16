//! Parse-once mutable project staging.

use std::collections::BTreeMap;

use super::{
    ProjectInput, ProjectInputError, ProjectReport, ResolutionRequest, ResolutionRequestKey,
    ResolutionResult, ResolutionTable, SourceFile, SourceTable,
    input::{normalize_relative, normalize_resolution_key, normalize_result, normalize_root},
};

pub struct ProjectSession<'a> {
    pub(super) linter: &'a crate::Linter,
    pub(super) root: std::path::PathBuf,
    pub(super) sources: SourceTable,
    pub(super) resolutions: ResolutionTable,
    pub(super) authored_requests: BTreeMap<ResolutionRequestKey, ResolutionRequest>,
    pub(super) analyzed: BTreeMap<
        String,
        (
            swc_common::sync::Lrc<swc_common::SourceMap>,
            crate::analysis::LocalModuleModel,
        ),
    >,
    pub(super) parse_diagnostics: BTreeMap<String, crate::ParseDiagnostic>,
}

impl<'a> ProjectSession<'a> {
    pub fn new(
        linter: &'a crate::Linter,
        root: impl Into<std::path::PathBuf>,
    ) -> Result<Self, ProjectInputError> {
        Ok(Self {
            linter,
            root: normalize_root(&root.into())?,
            sources: SourceTable::default(),
            resolutions: ResolutionTable::default(),
            authored_requests: BTreeMap::new(),
            analyzed: BTreeMap::new(),
            parse_diagnostics: BTreeMap::new(),
        })
    }

    pub fn add_source(
        &mut self,
        mut source: SourceFile,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        source.path = normalize_relative(&source.path)?;
        let path = source.path.clone();
        self.sources.insert(source)?;
        let source = self.sources.get(&path).expect("source was just inserted");
        match crate::parse::parse_with_language(&source.source, &source.path, source.language) {
            Ok(parsed) => {
                let local = crate::analysis::LocalModuleModel::analyze(
                    &parsed.program,
                    self.linter.analysis_environment(),
                );
                let requests = local
                    .interface()
                    .authored_requests(&path, &parsed.source_map);
                for request in &requests {
                    self.authored_requests
                        .insert(request.key.clone(), request.clone());
                }
                self.analyzed.insert(path, (parsed.source_map, local));
                Ok(requests)
            }
            Err(error) => {
                self.parse_diagnostics.insert(path, error);
                Ok(Vec::new())
            }
        }
    }

    pub fn record_resolution(
        &mut self,
        mut key: ResolutionRequestKey,
        mut result: ResolutionResult,
    ) -> Result<(), ProjectInputError> {
        normalize_resolution_key(&mut key)?;
        if !self.authored_requests.contains_key(&key) {
            return Err(ProjectInputError::UnknownRequest(key));
        }
        normalize_result(&mut result)?;
        self.resolutions.insert(key, result)
    }

    pub fn finish(self) -> Result<ProjectReport, ProjectInputError> {
        self.finish_with_timings().map(|(report, _, _)| report)
    }

    pub fn finish_with_timings(
        self,
    ) -> Result<(ProjectReport, std::time::Duration, std::time::Duration), ProjectInputError> {
        let input = ProjectInput {
            root: self.root,
            sources: self.sources.into_values().collect(),
            resolutions: self.resolutions.into_values().collect(),
        }
        .validate()?;
        self.linter
            .lint_analyzed_project_timed(input, self.analyzed, self.parse_diagnostics)
    }
}
