//! Targeted construction verifies the exact shared premises without a Cartesian join.
#[allow(dead_code)]
mod support;
use inference_engine::reasoning::{
    Evidence, Limit, PropositionLimits, Rdf12PropositionConstructor, ReasoningError,
    ReasoningStore, TripleSource,
};
use std::num::NonZeroU64;
use support::{Store, TYPE};
fn n(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap()
}
fn constructor(inferred: u64, matches: u64) -> Rdf12PropositionConstructor {
    Rdf12PropositionConstructor::new(PropositionLimits::new(n(inferred), n(matches)))
}
fn source() -> Store {
    let mut store = Store::new([]);
    let resource = store.iri("http://www.w3.org/2000/01/rdf-schema#Resource");
    let property = store.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#Property");
    for member in 10..30 {
        store.asserted.insert([member, TYPE, resource]);
    }
    for predicate in 50..60 {
        store.asserted.insert([predicate, TYPE, property]);
    }
    store
}
async fn candidates(rows: &[[u64; 3]], late: bool) -> support::Cursor {
    let mut store = Store::new(rows.iter().copied());
    store.fail_cursor = late;
    store
        .snapshot()
        .await
        .unwrap()
        .scan([None; 3])
        .await
        .unwrap()
}
#[tokio::test]
async fn constructs_only_selected_candidates_with_all_three_exact_shared_premises() {
    let mut store = source();
    let mut rows = candidates(&[[10, 50, 11], [11, 50, 10]], false).await;
    let report = constructor(2, 8)
        .run_candidates(&mut store, &mut rows)
        .await
        .unwrap();
    assert_eq!(report.inferred, 2);
    assert_eq!(report.matches, 8);
    assert_eq!(store.snapshot_calls, 1);
    assert_eq!(store.constructed.len(), 2);
    assert!(store.constructed.contains_key(&[10, 50, 11]));
    assert!(!store.constructed.contains_key(&[12, 51, 13]));
    assert!(!store.inferred.contains(&[10, 50, 11]));
    for inference in &store.evidence {
        let Evidence::Proposition { premises, .. } = &inference.evidence else {
            panic!("lost shared premise evidence")
        };
        assert!(
            premises
                .iter()
                .all(|triple| store.asserted.contains(triple))
        );
    }
}
#[tokio::test]
async fn absent_premises_do_not_construct_and_duplicates_do_not_spend_output_budget() {
    let mut store = source();
    let mut rows = candidates(&[[10, 50, 11], [10, 99, 11], [99, 50, 11]], false).await;
    let report = constructor(1, 20)
        .run_candidates(&mut store, &mut rows)
        .await
        .unwrap();
    assert_eq!(report.inferred, 1);
    assert_eq!(store.constructed.len(), 1);
    let mut again = candidates(&[[10, 50, 11]], false).await;
    assert_eq!(
        constructor(1, 4)
            .run_candidates(&mut store, &mut again)
            .await
            .unwrap()
            .inferred,
        0
    );
}
#[tokio::test]
async fn late_candidate_and_premise_errors_invalidate_partial_staging() {
    let mut store = source();
    let mut rows = candidates(&[[10, 50, 11], [11, 50, 10]], true).await;
    assert!(matches!(
        constructor(2, 20)
            .run_candidates(&mut store, &mut rows)
            .await,
        Err(ReasoningError::Storage(_))
    ));
    assert_eq!(store.evidence.len(), 2);
    let mut store = source();
    store.fail_cursor = true;
    let mut rows = candidates(&[[10, 50, 11]], false).await;
    assert!(matches!(
        constructor(1, 20)
            .run_candidates(&mut store, &mut rows)
            .await,
        Err(ReasoningError::Storage(_))
    ));
    assert!(store.evidence.is_empty());
}
#[tokio::test]
async fn work_output_cancellation_and_substituted_depth_limits_never_report_completion() {
    for failure in 0..4 {
        let mut store = source();
        let tuple = if failure == 3 {
            store.term_depths.insert(10, 4);
            [10, 50, 11]
        } else {
            [10, 50, 11]
        };
        store.cancelled = failure == 2;
        let mut rows = candidates(&[tuple, [11, 50, 10]], false).await;
        let error = constructor(
            if failure == 1 { 1 } else { 2 },
            if failure == 0 { 1 } else { 20 },
        )
        .run_candidates(&mut store, &mut rows)
        .await
        .unwrap_err();
        assert!(matches!(
            (failure, error),
            (0, ReasoningError::LimitExceeded(Limit::Matches))
                | (1, ReasoningError::LimitExceeded(Limit::Inferred))
                | (2, ReasoningError::Cancelled)
                | (3, ReasoningError::Storage(_))
        ));
    }
}

#[tokio::test]
async fn malformed_exact_probe_is_a_hard_error_instead_of_false_absence() {
    let mut store = source();
    store.mismatched_probe = true;
    let mut rows = candidates(&[[10, 50, 11]], false).await;
    assert!(matches!(
        constructor(1, 20)
            .run_candidates(&mut store, &mut rows)
            .await,
        Err(ReasoningError::InvalidProbe)
    ));
    assert!(store.inferred.is_empty());
}
