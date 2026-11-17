//! RDF 1.2 graph matching must share one blank-node mapping across nested terms.
#[allow(
    dead_code,
    reason = "the capped graph fixture is shared with rule-execution tests"
)]
mod support;
use inference_engine::{
    entailment::{EntailmentError, EntailmentLimits, EntailmentTerms, SimpleEntailment},
    reasoning::ReasoningStore,
};
use std::{
    io,
    num::{NonZeroU32, NonZeroU64},
};
use support::Store;

struct Terms {
    cancelled: bool,
}
impl EntailmentTerms for Terms {
    type Term = u64;
    type Error = io::Error;
    async fn is_blank(&self, term: &u64) -> io::Result<bool> {
        Ok((100..200).contains(term))
    }
    async fn triple_term(&self, term: &u64) -> io::Result<Option<[u64; 3]>> {
        Ok(match term {
            300 => Some([100, 2, 101]),
            301 => Some([10, 2, 11]),
            302 => Some([10, 2, 12]),
            303 => Some([300, 3, 100]),
            304 => Some([301, 3, 10]),
            _ => None,
        })
    }
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

fn limits(patterns: u32, bindings: u32, terms: u32, steps: u64) -> EntailmentLimits {
    EntailmentLimits::new(
        NonZeroU32::new(patterns).unwrap(),
        NonZeroU32::new(bindings).unwrap(),
        NonZeroU32::new(terms).unwrap(),
        NonZeroU64::new(steps).unwrap(),
    )
}

async fn entails(input: &[[u64; 3]], target: &[[u64; 3]]) -> bool {
    let source = Store::new(input.iter().copied()).snapshot().await.unwrap();
    let target = Store::new(target.iter().copied()).snapshot().await.unwrap();
    SimpleEntailment::new(limits(64, 192, 64, 100_000))
        .check(&source, &target, &Terms { cancelled: false })
        .await
        .unwrap()
        .entailed
}

#[tokio::test]
async fn shares_blank_mapping_across_triples_and_backtracks_over_candidates() {
    assert!(
        entails(
            &[[10, 1, 11], [20, 1, 21], [20, 2, 22]],
            &[[100, 1, 101], [100, 2, 102]]
        )
        .await
    );
    assert!(!entails(&[[10, 1, 11], [20, 2, 22]], &[[100, 1, 101], [100, 2, 102]]).await);
    assert!(!entails(&[[10, 1, 11]], &[[100, 1, 100]]).await);
    assert!(
        entails(&[[10, 1, 10]], &[[100, 1, 101]]).await,
        "mapping need not be injective"
    );
}

#[tokio::test]
async fn substitutes_blanks_inside_nested_triple_terms_and_outer_triples_consistently() {
    assert!(entails(&[[20, 1, 304], [10, 4, 11]], &[[20, 1, 303], [100, 4, 101]]).await);
    assert!(!entails(&[[20, 1, 304], [12, 4, 11]], &[[20, 1, 303], [100, 4, 101]]).await);
    assert!(!entails(&[[20, 1, 302], [10, 4, 11]], &[[20, 1, 300], [100, 4, 101]]).await);
}

#[tokio::test]
async fn triple_terms_do_not_assert_their_contents_or_replace_unrelated_constants() {
    assert!(!entails(&[[20, 1, 301]], &[[10, 2, 11]]).await);
    assert!(!entails(&[[20, 1, 301]], &[[20, 1, 302]]).await);
    assert!(entails(&[[20, 1, 301]], &[[20, 1, 100]]).await);
    assert!(entails(&[], &[]).await);
    assert!(!entails(&[], &[[10, 1, 11]]).await);
}

#[tokio::test]
async fn resource_limits_are_errors_instead_of_false_non_entailment() {
    let source = Store::new([[10, 1, 11], [20, 1, 304]])
        .snapshot()
        .await
        .unwrap();
    for (target, budget, expected) in [
        (
            vec![[100, 1, 101], [100, 2, 102]],
            limits(1, 192, 64, 1000),
            "target",
        ),
        (vec![[100, 1, 101]], limits(64, 1, 64, 1000), "bindings"),
        (vec![[20, 1, 303]], limits(64, 192, 1, 1000), "term"),
        (vec![[10, 1, 11]], limits(64, 192, 64, 1), "steps"),
    ] {
        // bounded: each resource-exhaustion target contains at most two tuples.
        let target = Store::new(target).snapshot().await.unwrap();
        let error = SimpleEntailment::new(budget)
            .check(&source, &target, &Terms { cancelled: false })
            .await
            .unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[tokio::test]
async fn cancellation_and_late_cursor_failures_never_become_false_or_empty_success() {
    let source = Store::new([]).snapshot().await.unwrap();
    let target = Store::new([]).snapshot().await.unwrap();
    assert!(matches!(
        SimpleEntailment::new(limits(64, 192, 64, 1000))
            .check(&source, &target, &Terms { cancelled: true })
            .await,
        Err(EntailmentError::Cancelled)
    ));
    let mut store = Store::new([[10, 1, 11]]);
    store.fail_cursor = true;
    let source = store.snapshot().await.unwrap();
    let target = Store::new([[100, 2, 101]]).snapshot().await.unwrap();
    assert!(matches!(
        SimpleEntailment::new(limits(64, 192, 64, 1000))
            .check(&source, &target, &Terms { cancelled: false })
            .await,
        Err(EntailmentError::Storage(_))
    ));
}
