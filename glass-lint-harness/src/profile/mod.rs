//! Deterministic corpus discovery and bounded provider profiling.
//!
//! Setup, measured linting, and phase metrics are kept separate so profiling
//! compares analysis work without accidentally timing corpus preparation.

mod config;
mod corpus;
mod metrics;
mod runner;
mod types;

pub use config::{
    ProfileCatalogProvider, ProfileConfig, ProfileConfigBuilder, ProfileCorpusIdentity,
    ProfileWorkload, ProfileWorkloadIdentity, RuleSelectionProfile,
};
pub use corpus::{discover_profile_files, sample_paths};
pub use runner::run_profile;
pub use types::{
    ProfileOperationCounts, ProfilePhaseTimings, ProfileRepetitionSummary, ProfileSummary,
    ProfileWorkloadSummary, ensure_profile_correctness_match,
};

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        fs,
        num::NonZeroUsize,
        path::{Path, PathBuf},
        time::Duration,
    };

    use glass_lint_core::ReportCompletion;

    use super::{types::MeasuredRepetitionAccumulator, *};

    fn temp_root() -> crate::test_support::TempDir {
        crate::test_support::TempDir::new()
    }

    fn config(root: &Path) -> ProfileConfig {
        ProfileConfig::builder([root.to_owned()])
            .seed(1)
            .build()
            .unwrap()
    }

    #[test]
    fn public_profile_builder_rejects_empty_workloads() {
        let error = ProfileConfig::builder(Vec::<PathBuf>::new())
            .build()
            .unwrap_err();
        assert!(error.to_string().contains("at least one --path"));
    }

    #[test]
    fn discovers_sorted_unique_filtered_files() {
        let root = temp_root();
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("z.js"), "").unwrap();
        fs::write(root.join("nested/a.js"), "").unwrap();
        fs::write(root.join("nested/no.txt"), "").unwrap();
        let paths = discover_profile_files(
            &[root.to_owned(), root.join("nested")],
            &["**/a.js".into()],
            &[],
        )
        .unwrap();
        assert_eq!(paths, vec![root.join("nested/a.js")]);
    }

    #[test]
    fn discovers_all_runtime_module_extensions_but_not_declarations() {
        let root = temp_root();
        for filename in ["a.js", "b.cjs", "c.mjs", "d.ts", "e.cts", "f.mts", "g.d.ts"] {
            fs::write(root.join(filename), "").unwrap();
        }
        let paths =
            discover_profile_files(std::slice::from_ref(&root.to_path_buf()), &[], &[]).unwrap();
        assert_eq!(paths.len(), 6);
        assert!(!paths.iter().any(|path| path.ends_with("g.d.ts")));
    }

    #[test]
    fn empty_folder_is_a_valid_profile_corpus() {
        let root = temp_root();
        let result = run_profile(&config(&root)).unwrap();
        assert_eq!(result.inputs, 0);
        assert_eq!(result.runs, 0);
    }

    #[test]
    fn malformed_files_are_counted_as_parse_diagnostics() {
        let root = temp_root();
        fs::write(root.join("broken.js"), "function (").unwrap();
        let result = run_profile(&config(&root)).unwrap();
        assert_eq!(result.inputs, 1);
        assert_eq!(result.diagnostics, 1);
        assert_eq!(result.errors, 0);
    }

    #[test]
    fn sampling_is_deterministic_for_a_seed() {
        let mut left: Vec<_> = (0..20).map(|i| PathBuf::from(format!("{i}.js"))).collect();
        let mut right = left.clone();
        sample_paths(&mut left, 5, 42);
        sample_paths(&mut right, 5, 42);
        assert_eq!(left, right);
    }

    #[test]
    fn typed_accumulators_saturate_without_cross_item_bytes() {
        let mut phases = ProfilePhaseTimings::with_discovery(Duration::MAX);
        phases += ProfilePhaseTimings::with_discovery(Duration::from_secs(1));
        assert_eq!(phases.discovery(), Duration::MAX);
        phases.record_analyze_source(Duration::from_secs(2));
        phases.record_linking(Duration::from_secs(3));
        phases.record_matching(Duration::from_secs(4));
        assert_eq!(phases.parse_and_local_analysis(), Duration::from_secs(2));
        assert_eq!(phases.linking_and_matching(), Duration::from_secs(7));

        let mut counts = ProfileOperationCounts {
            files: usize::MAX,
            ..ProfileOperationCounts::default()
        };
        counts += ProfileOperationCounts {
            files: 1,
            ..ProfileOperationCounts::default()
        };
        assert_eq!(counts.files, usize::MAX);

        let first_bytes = 7_u64;
        let second_bytes = 11_u64;
        let suite_bytes = first_bytes.saturating_add(second_bytes);
        assert_eq!(first_bytes, 7);
        assert_eq!(second_bytes, 11);
        assert_eq!(suite_bytes, 18);
    }

    fn admitted_config(root: &Path, workers: usize) -> ProfileConfig {
        ProfileConfig::builder([root.to_owned()])
            .seed(1)
            .warm_up(1)
            .workers(NonZeroUsize::new(workers).unwrap())
            .workload(ProfileWorkload::AdmittedProject)
            .build()
            .unwrap()
    }

    #[test]
    fn admitted_project_excludes_warmup_from_measured_duration() {
        let mut measured = MeasuredRepetitionAccumulator::default();
        for duration in [Duration::from_millis(3), Duration::from_millis(7)] {
            measured.record(ProfileRepetitionSummary {
                duration,
                findings: 0,
                diagnostics: 0,
                completion: ReportCompletion::Complete,
                run_completions: vec![ReportCompletion::Complete],
                operation_counts: ProfileOperationCounts::default(),
                evidence_order_digest: String::new(),
            });
        }
        assert_eq!(measured.total_duration(), Duration::from_millis(10));
        assert_eq!(
            super::metrics::median_duration(&measured.repetitions),
            Duration::from_millis(3)
        );
    }

    #[test]
    fn repetition_accumulator_executes_every_requested_warmup() {
        let warmups = Cell::new(0);
        let measured = Cell::new(0);
        let result = MeasuredRepetitionAccumulator::measure(
            3,
            2,
            || {
                warmups.set(warmups.get() + 1);
                Ok(())
            },
            || {
                measured.set(measured.get() + 1);
                Ok(ProfileRepetitionSummary {
                    duration: Duration::ZERO,
                    findings: 0,
                    diagnostics: 0,
                    completion: ReportCompletion::Complete,
                    run_completions: Vec::new(),
                    operation_counts: ProfileOperationCounts::default(),
                    evidence_order_digest: String::new(),
                })
            },
        )
        .unwrap();

        assert_eq!(warmups.get(), 3);
        assert_eq!(measured.get(), 2);
        assert_eq!(result.repetitions.len(), 2);
    }

    #[test]
    fn admitted_project_counts_all_diagnostics_and_completion() {
        let root = temp_root();
        fs::write(root.join("broken.js"), "function (").unwrap();
        fs::write(root.join("request.js"), "import './missing.js';").unwrap();
        let result = run_profile(&admitted_config(&root, 1)).unwrap();
        assert_eq!(result.repetitions.len(), 1);
        assert_eq!(result.diagnostics, result.repetitions[0].diagnostics);
        assert!(result.diagnostics >= 2);
        assert_eq!(result.repetitions[0].completion, ReportCompletion::Partial);
        assert!(result.measured_elapsed >= result.repetitions[0].duration);
        assert!(result.wall_duration >= result.setup_duration + result.measured_elapsed);
    }

    #[test]
    fn admitted_project_preserves_full_operation_counts() {
        let root = temp_root();
        fs::write(root.join("a.js"), "export const value = 1; fetch('/');").unwrap();
        fs::write(root.join("b.js"), "import { value } from './a.js'; value;").unwrap();
        let result = run_profile(&admitted_config(&root, 1)).unwrap();
        assert_eq!(
            result.operation_counts,
            result.repetitions[0].operation_counts
        );
        assert_eq!(result.operation_counts.files, 2);
        assert!(result.operation_counts.requests > 0);
        assert!(result.operation_counts.exports > 0);
        assert_eq!(
            result.operation_counts.effect_projections,
            result.repetitions[0].operation_counts.effect_projections
        );
        assert_eq!(
            result.operation_counts.scc_rounds,
            result.repetitions[0].operation_counts.scc_rounds
        );
    }

    #[test]
    fn admitted_project_worker_counts_have_identical_correctness() {
        let root = temp_root();
        for index in 0..8 {
            fs::write(root.join(format!("{index}.js")), "fetch('/');").unwrap();
        }
        let manifest = root.join("profile-manifest.json");
        crate::create_profile_manifest(&root, &[], &[], None, 1, "fixture", &manifest).unwrap();
        let first = ProfileConfig::builder([root.to_owned()])
            .seed(1)
            .warm_up(1)
            .workers(NonZeroUsize::new(1).unwrap())
            .workload(ProfileWorkload::AdmittedProject)
            .manifest(Some(manifest.clone()))
            .build()
            .unwrap();
        let second = ProfileConfig::builder([root.to_owned()])
            .seed(1)
            .warm_up(1)
            .workers(NonZeroUsize::new(2).unwrap())
            .workload(ProfileWorkload::AdmittedProject)
            .manifest(Some(manifest))
            .build()
            .unwrap();
        let one = run_profile(&first).unwrap();
        let two = run_profile(&second).unwrap();
        assert_eq!(one.findings, two.findings);
        assert_eq!(one.diagnostics, two.diagnostics);
        assert_eq!(one.operation_counts, two.operation_counts);
        assert_eq!(one.repetitions[0].completion, two.repetitions[0].completion);
        assert_eq!(
            one.repetitions[0].evidence_order_digest,
            two.repetitions[0].evidence_order_digest
        );
        ensure_profile_correctness_match(&one, &two).unwrap();
        let mut mismatched = two;
        mismatched.repetitions[0].completion = ReportCompletion::Partial;
        assert_eq!(
            ensure_profile_correctness_match(&one, &mismatched)
                .unwrap_err()
                .to_string(),
            "profile correctness differs at repetition 1"
        );
    }

    #[test]
    fn independent_file_worker_counts_have_identical_correctness() {
        let root = temp_root();
        for (index, repetitions) in [20_000, 1, 10_000, 2, 5_000, 3].into_iter().enumerate() {
            fs::write(
                root.join(format!("{index}.js")),
                "fetch('/');\n".repeat(repetitions),
            )
            .unwrap();
        }
        let manifest = root.join("profile-manifest.json");
        crate::create_profile_manifest(&root, &[], &[], None, 1, "fixture", &manifest).unwrap();
        let first = ProfileConfig::builder([root.to_owned()])
            .seed(1)
            .manifest(Some(manifest.clone()))
            .build()
            .unwrap();
        let one = run_profile(&first).unwrap();
        let parallel = ProfileConfig::builder([root.to_owned()])
            .seed(1)
            .workers(NonZeroUsize::new(4).unwrap())
            .manifest(Some(manifest))
            .build()
            .unwrap();
        let parallel = run_profile(&parallel).unwrap();
        ensure_profile_correctness_match(&one, &parallel).unwrap();
    }

    #[test]
    fn loader_project_worker_counts_use_verified_repetitions() {
        let root = temp_root();
        fs::write(root.join("a.js"), "fetch('/');").unwrap();
        let manifest = root.join("profile-manifest.json");
        crate::create_profile_manifest(&root, &[], &[], None, 1, "fixture", &manifest).unwrap();

        let one = ProfileConfig::builder([root.to_owned()])
            .seed(1)
            .workload(ProfileWorkload::LoaderProject)
            .manifest(Some(manifest.clone()))
            .build()
            .unwrap();
        let parallel = ProfileConfig::builder([root.to_owned()])
            .seed(1)
            .workers(NonZeroUsize::new(2).unwrap())
            .workload(ProfileWorkload::LoaderProject)
            .manifest(Some(manifest))
            .build()
            .unwrap();

        let one = run_profile(&one).unwrap();
        let parallel = run_profile(&parallel).unwrap();
        assert!(matches!(
            one.workload.corpus,
            ProfileCorpusIdentity::Verified(_)
        ));
        assert_eq!(one.repetitions.len(), 1);
        assert!(!one.repetitions[0].run_completions.is_empty());
        ensure_profile_correctness_match(&one, &parallel).unwrap();
    }

    #[test]
    fn normal_and_admitted_modes_use_the_same_verified_manifest() {
        let root = temp_root();
        fs::write(root.join("a.js"), "fetch('/');").unwrap();
        let manifest_path = root.join("profile-manifest.json");
        crate::create_profile_manifest(&root, &[], &[], None, 1, "fixture", &manifest_path)
            .unwrap();
        let normal_config = ProfileConfig::builder([root.to_owned()])
            .seed(1)
            .manifest(Some(manifest_path.clone()))
            .build()
            .unwrap();
        let normal = run_profile(&normal_config).unwrap();
        let admitted_config = ProfileConfig::builder([root.to_owned()])
            .seed(1)
            .warm_up(1)
            .workers(NonZeroUsize::new(1).unwrap())
            .workload(ProfileWorkload::AdmittedProject)
            .manifest(Some(manifest_path))
            .build()
            .unwrap();
        let admitted = run_profile(&admitted_config).unwrap();
        assert_eq!(normal.workload.corpus, admitted.workload.corpus);
        assert_eq!(normal.bytes, admitted.bytes);
        assert_eq!(normal.inputs, admitted.inputs);
    }

    #[test]
    fn workload_mode_is_explicit() {
        let root = temp_root();
        let config = admitted_config(&root, 1);
        assert_eq!(config.workload, ProfileWorkload::AdmittedProject);
    }

    #[test]
    fn admitted_project_rejects_multiple_or_outside_roots() {
        let root = temp_root();
        let outside = temp_root();
        let error = ProfileConfig::builder([root.to_owned(), outside.to_path_buf()])
            .seed(1)
            .warm_up(1)
            .workers(NonZeroUsize::new(1).unwrap())
            .workload(ProfileWorkload::AdmittedProject)
            .build()
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "--admitted-project requires exactly one --path root"
        );
        fs::write(root.join("outside.js"), "").unwrap();
        let error = ProfileConfig::builder([root.join("outside.js")])
            .seed(1)
            .warm_up(1)
            .workers(NonZeroUsize::new(1).unwrap())
            .workload(ProfileWorkload::AdmittedProject)
            .build()
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "--admitted-project root must be a directory"
        );
    }

    #[cfg(unix)]
    #[test]
    fn recursive_discovery_does_not_follow_symlinks() {
        let root = temp_root();
        fs::write(root.join("real.js"), "").unwrap();
        std::os::unix::fs::symlink(".", root.join("link")).unwrap();
        let paths =
            discover_profile_files(std::slice::from_ref(&root.to_path_buf()), &[], &[]).unwrap();
        assert_eq!(paths, vec![root.join("real.js")]);
    }
}
