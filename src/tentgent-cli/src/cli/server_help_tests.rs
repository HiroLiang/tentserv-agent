use super::app::Cli;
use clap::Parser;

#[test]
fn server_help_explains_eager_idle_and_image_cloud_exceptions() {
    let error = Cli::try_parse_from(["tentgent", "server", "run", "--help"]).unwrap_err();
    assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
    let help = error.to_string();
    for expected in [
        "validates base-model loading before readiness",
        "model idle 0",
        "image-generation",
        "requires --lazy-load",
        "Cloud targets reject",
        "observation expiry does not cancel",
    ] {
        assert!(help.contains(expected), "missing {expected}: {help}");
    }
    assert!(!help.contains("Eager startup is pending"));
}

#[test]
fn cluster_help_explains_lazy_versus_all_local_eager_validation() {
    let error = Cli::try_parse_from(["tentgent", "cluster", "run", "--help"]).unwrap_err();
    assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
    assert!(error
        .to_string()
        .contains("validating all local routes before ready"));
}
