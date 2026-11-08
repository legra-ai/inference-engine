//! Public profile compilation and unsupported-selection regressions.
use inference_engine::compile_profile;

#[test]
fn compiles_checked_shared_rules_without_claiming_execution() {
    let profile = compile_profile("rdfs-core").unwrap();
    assert_eq!(profile.program.rules.len(), 14);
    profile.program.validate().unwrap();
    assert!(!profile.exclusions.is_empty());
}

#[test]
fn unsupported_full_profile_fails_instead_of_downgrading() {
    assert!(compile_profile("owl2-rl").is_err());
}
