//! The declared rule authority must follow the exact compiled dependency pin.

#[path = "../build_support.rs"]
mod build_support;

use inference_engine::reasoning::{ExecutionProfile, Reasoner, RunLimits};
use std::num::{NonZeroU32, NonZeroU64};

#[test]
fn dependency_updates_change_catalogue_identity_without_a_second_version_literal() {
    for manifest in [
        "[dependencies]\nrdf-reasoning = '=9.8.7'\n",
        "[dependencies.rdf-reasoning]\nversion = '=9.8.7'\n",
    ] {
        assert_eq!(
            build_support::rule_catalogue(manifest).unwrap(),
            "rdf-reasoning/9.8.7"
        );
    }
}

#[test]
fn refuses_ambiguous_dependency_identities() {
    for manifest in [
        "[dependencies]\nrdf-reasoning = '^2.7'\n",
        "[dependencies]\nrdf-reasoning = '*'\n",
        "[dependencies.rdf-reasoning]\nworkspace = true\n",
        "[dependencies]\n",
    ] {
        assert!(build_support::rule_catalogue(manifest).is_err());
    }
}

#[test]
fn runtime_identity_matches_the_actual_package_manifest() {
    let reasoner = Reasoner::new(
        ExecutionProfile::RdfsCore,
        RunLimits::new(NonZeroU32::MIN, NonZeroU64::MIN, NonZeroU64::MIN),
    );
    assert_eq!(
        reasoner.rule_catalogue(),
        build_support::rule_catalogue(include_str!("../Cargo.toml")).unwrap()
    );
}
