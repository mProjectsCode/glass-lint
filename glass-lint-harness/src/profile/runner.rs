use anyhow::Result;

use crate::profile::{
    config::{ProfileConfig, ProfileWorkload, validate_config},
    types::ProfileSummary,
};

mod admitted;
mod files;
mod projects;
mod summary;
mod support;
mod workers;

pub fn run_profile(config: &ProfileConfig) -> Result<ProfileSummary> {
    validate_config(config)?;
    match config.workload {
        ProfileWorkload::Files => files::run(config),
        ProfileWorkload::LoaderProject => projects::run(config),
        ProfileWorkload::AdmittedProject => admitted::run(config),
    }
}
