//! Executable two-rule RDFS contract against a bounded test-only store.

mod support;
use inference_engine::reasoning::{Evidence, ExecutionProfile, Inference, Reasoner, RunLimits};
use std::collections::BTreeSet;
use support::{SUBCLASS, Store, TYPE, limits};

fn reasoner() -> Reasoner {
    Reasoner::new(ExecutionProfile::RdfsSubclass, limits(100, 20, 10_000))
}

#[tokio::test]
async fn derives_transitive_subclasses_and_inherited_membership() {
    let mut store = Store::new([[10, SUBCLASS, 11], [11, SUBCLASS, 12], [20, TYPE, 10]]);
    let report = reasoner().run(&mut store).await.unwrap();
    assert_eq!(
        store.inferred,
        BTreeSet::from([[10, SUBCLASS, 12], [20, TYPE, 11], [20, TYPE, 12]])
    );
    assert_eq!(report.inferred, 3);
    assert_eq!(store.asserted.len(), 3);
    assert!(store.evidence.iter().all(|evidence| {
        let Evidence::Rule { premises, .. } = &evidence.evidence else {
            return false;
        };
        premises
            .iter()
            .all(|p| store.asserted.contains(p) || store.inferred.contains(p))
    }));
}

#[tokio::test]
async fn cycles_terminate_and_assertions_win_duplicates() {
    let mut store = Store::new([
        [10, SUBCLASS, 11],
        [11, SUBCLASS, 10],
        [20, TYPE, 10],
        [20, TYPE, 11],
    ]);
    reasoner().run(&mut store).await.unwrap();
    assert_eq!(
        store.inferred,
        BTreeSet::from([[10, SUBCLASS, 10], [11, SUBCLASS, 11]])
    );
}

#[tokio::test]
async fn never_reverses_subclass_membership() {
    let mut store = Store::new([[10, SUBCLASS, 11], [20, TYPE, 11]]);
    reasoner().run(&mut store).await.unwrap();
    assert!(store.inferred.is_empty());
}

#[tokio::test]
async fn late_cursor_and_write_failures_leave_only_unpublished_staging() {
    let mut store = Store::new([[10, SUBCLASS, 11], [20, TYPE, 10]]);
    store.fail_cursor = true;
    let error = reasoner().run(&mut store).await.unwrap_err();
    assert!(error.to_string().contains("cursor failed"));
    assert_eq!(store.inferred.len(), 1);

    let mut store = Store::new([[10, SUBCLASS, 11], [11, SUBCLASS, 12], [20, TYPE, 10]]);
    store.fail_after_writes = Some(1);
    let error = reasoner().run(&mut store).await.unwrap_err();
    assert!(error.to_string().contains("write failed"));
    assert_eq!(store.inferred.len(), 1);
}

#[tokio::test]
async fn exact_output_limit_can_still_prove_fixpoint_and_empty_input_is_valid() {
    let mut store = Store::new([[10, SUBCLASS, 11], [11, SUBCLASS, 12], [20, TYPE, 10]]);
    let report = Reasoner::new(ExecutionProfile::RdfsSubclass, limits(3, 20, 10_000))
        .run(&mut store)
        .await
        .unwrap();
    assert_eq!(report.inferred, 3);
    let report = reasoner().run(&mut Store::default()).await.unwrap();
    assert_eq!((report.rounds, report.inferred), (1, 0));
}

#[test]
fn portable_execution_values_round_trip_without_a_json_intermediate() {
    let evidence = Inference {
        triple: [10, SUBCLASS, 12],
        evidence: Evidence::Rule {
            rule: rdf_reasoning::RuleId::Rdfs11,
            premises: [[10, SUBCLASS, 11], [11, SUBCLASS, 12]].into(),
        },
    };
    let json = serde_json::to_vec(&evidence).unwrap();
    assert_eq!(
        serde_json::from_slice::<Inference<u64>>(&json).unwrap(),
        evidence
    );
    let binary = postcard::to_allocvec(&evidence).unwrap();
    assert_eq!(
        postcard::from_bytes::<Inference<u64>>(&binary).unwrap(),
        evidence
    );
    assert!(serde_json::from_str::<RunLimits>(r#"{"rounds":0,"inferred":1,"matches":1}"#).is_err());
    assert!(postcard::from_bytes::<RunLimits>(&[0, 1, 1]).is_err());
    assert!(
        serde_json::from_str::<RunLimits>(
            r#"{"rounds":1,"inferred":1,"matches":1,"fallback":true}"#
        )
        .is_err()
    );
    assert!(serde_json::from_str::<Inference<u64>>(r#"{"triple":[10,2,12],"evidence":{"Rule":{"rule":"rdfs11","premises":{"Two":[[10,2,11],[11,2,12]]}}},"source":"ignored"}"#).is_err());
}

#[tokio::test]
async fn recomputation_after_deletion_does_not_seed_old_inference() {
    let mut before = Store::new([[10, SUBCLASS, 11], [11, SUBCLASS, 12], [20, TYPE, 10]]);
    reasoner().run(&mut before).await.unwrap();
    assert!(before.inferred.contains(&[20, TYPE, 12]));
    let mut after = Store::new([[10, SUBCLASS, 11], [20, TYPE, 10]]);
    reasoner().run(&mut after).await.unwrap();
    assert_eq!(after.inferred, BTreeSet::from([[20, TYPE, 11]]));
}

#[tokio::test]
async fn budgets_fail_without_inserting_beyond_the_limit() {
    let mut store = Store::new([[10, SUBCLASS, 11], [11, SUBCLASS, 12], [20, TYPE, 10]]);
    let error = Reasoner::new(ExecutionProfile::RdfsSubclass, limits(1, 20, 10_000))
        .run(&mut store)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("inferred"));
    assert_eq!(store.inferred.len(), 1);
    for (rounds, matches) in [(1, 10_000), (20, 1)] {
        let mut store = Store::new([[10, SUBCLASS, 11], [11, SUBCLASS, 12], [20, TYPE, 10]]);
        assert!(
            Reasoner::new(ExecutionProfile::RdfsSubclass, limits(100, rounds, matches))
                .run(&mut store)
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn cancellation_and_storage_errors_are_not_success() {
    for (cancelled, fail_read, fail_write) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let mut store = Store::new([[10, SUBCLASS, 11], [20, TYPE, 10]]);
        store.cancelled = cancelled;
        store.fail_read = fail_read;
        store.fail_write = fail_write;
        assert!(reasoner().run(&mut store).await.is_err());
    }
}

#[test]
fn unsupported_profiles_are_not_silently_weakened() {
    assert_eq!(
        "rdfs-subclass".parse::<ExecutionProfile>().unwrap(),
        ExecutionProfile::RdfsSubclass
    );
    for name in ["rdfs", "owl2-rl", "isSubclassOf"] {
        assert!(name.parse::<ExecutionProfile>().is_err());
    }
    assert_eq!(
        ExecutionProfile::RdfsSubclass.rules(),
        &[rdf_reasoning::RuleId::Rdfs9, rdf_reasoning::RuleId::Rdfs11]
    );
}
