use clap::CommandFactory;

use super::*;

#[test]
fn project_profile_modes_are_mutually_exclusive() {
    let error = Args::try_parse_from([
        "glass-lint-harness",
        "profile",
        "--path",
        ".",
        "--project",
        "--admitted-project",
    ])
    .err()
    .unwrap();
    assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn profile_help_documents_manifest_and_admitted_modes() {
    let mut command = Args::command();
    let profile = command.find_subcommand_mut("profile").unwrap();
    let help = profile.render_long_help().to_string();
    for option in [
        "--admitted-project",
        "--manifest",
        "--create-manifest",
        "--root-label",
    ] {
        assert!(help.contains(option), "missing {option} from profile help");
    }
}
