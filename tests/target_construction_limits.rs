//! Storage and resource failures must not become completed target-search reports.
#[allow(dead_code)]
mod source_support;
use inference_engine::{
    entailment::{EntailmentError, EntailmentLimits, EntailmentTerms},
    reasoning::{
        Limit, PropositionLimits, Rdf12TargetConstructor, Rdfs12Context, ReasoningError,
        ReasoningStore, TargetConstructionError, TargetConstructionLimits,
    },
};
use source_support::{Diagnostics, Fixture, MemoryStore, context_config};
use std::{
    io,
    num::{NonZeroU32, NonZeroU64},
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};
type Target = <MemoryStore as ReasoningStore>::Snapshot;
fn u32n(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
}
fn u64n(n: u64) -> NonZeroU64 {
    NonZeroU64::new(n).unwrap()
}
fn limits() -> TargetConstructionLimits {
    TargetConstructionLimits::new(
        EntailmentLimits::new(u32n(8), u32n(16), u32n(64), u64n(100_000)),
        PropositionLimits::new(u64n(100), u64n(100_000)),
        u32n(64),
        u64n(100_000),
    )
}
async fn fixture() -> (Fixture, Target) {
    let mut f = Fixture::new();
    let a = f.iri("urn:example:a");
    let p = f.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
    let b = f.iri("urn:example:b");
    let quoted = f.quoted([a, p, b]).await;
    f.store.inner.asserted.insert([a, p, quoted]);
    let proposition = f.iri("http://www.w3.org/2000/01/rdf-schema#Proposition");
    let target = MemoryStore::new([[quoted, p, proposition]])
        .snapshot()
        .await
        .unwrap();
    Rdfs12Context::new(context_config())
        .run(
            &mut f.store,
            &f.access,
            &target,
            &mut Diagnostics::default(),
        )
        .await
        .unwrap();
    (f, target)
}

struct CancellingTerms<'a> {
    inner: &'a source_support::Access,
    reads: AtomicU32,
    cancelled: AtomicBool,
}
impl EntailmentTerms for CancellingTerms<'_> {
    type Term = u64;
    type Error = io::Error;
    async fn is_blank(&self, key: &u64) -> io::Result<bool> {
        self.inner.is_blank(key).await
    }
    async fn triple_term(&self, key: &u64) -> io::Result<Option<[u64; 3]>> {
        let result = self.inner.triple_term(key).await?;
        if self.reads.fetch_add(1, Ordering::Relaxed) == 5 {
            self.cancelled.store(true, Ordering::Relaxed);
        }
        Ok(result)
    }
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed) || self.inner.is_cancelled()
    }
}

#[tokio::test]
async fn cancellation_on_the_final_goal_free_target_read_aborts() {
    let mut f = Fixture::new();
    let row = [f.iri("urn:s"), f.iri("urn:p"), f.iri("urn:o")];
    let target = MemoryStore::new([row]).snapshot().await.unwrap();
    let access = CancellingTerms {
        inner: &f.access,
        reads: AtomicU32::new(0),
        cancelled: AtomicBool::new(false),
    };
    let result = Rdf12TargetConstructor::new(limits())
        .run(&mut f.store, &access, &target)
        .await;
    assert!(matches!(
        result,
        Err(TargetConstructionError::Matching(
            EntailmentError::Cancelled
        ))
    ));
}

#[tokio::test]
async fn grounding_and_matching_caps_abort_without_truncating_search() {
    let (mut f, target) = fixture().await;
    let matching = EntailmentLimits::new(u32n(8), u32n(16), u32n(64), u64n(100_000));
    let propositions = PropositionLimits::new(u64n(100), u64n(100_000));
    let result = Rdf12TargetConstructor::new(TargetConstructionLimits::new(
        matching,
        propositions,
        u32n(1),
        u64n(100_000),
    ))
    .run(&mut f.store, &f.access, &target)
    .await;
    assert!(matches!(result, Err(TargetConstructionError::SlotLimit)));
    let result = Rdf12TargetConstructor::new(TargetConstructionLimits::new(
        matching,
        propositions,
        u32n(64),
        u64n(1),
    ))
    .run(&mut f.store, &f.access, &target)
    .await;
    assert!(matches!(result, Err(TargetConstructionError::WorkLimit)));
    let matching = EntailmentLimits::new(u32n(8), u32n(16), u32n(64), u64n(1));
    let result = Rdf12TargetConstructor::new(TargetConstructionLimits::new(
        matching,
        propositions,
        u32n(64),
        u64n(100_000),
    ))
    .run(&mut f.store, &f.access, &target)
    .await;
    assert!(matches!(
        result,
        Err(TargetConstructionError::Matching(
            EntailmentError::StepLimit
        ))
    ));
}

#[tokio::test]
async fn shared_constructor_budget_and_late_premise_errors_remain_hard_failures() {
    let (mut f, target) = fixture().await;
    let config = TargetConstructionLimits::new(
        EntailmentLimits::new(u32n(8), u32n(16), u32n(64), u64n(100_000)),
        PropositionLimits::new(u64n(100), u64n(1)),
        u32n(64),
        u64n(100_000),
    );
    let result = Rdf12TargetConstructor::new(config)
        .run(&mut f.store, &f.access, &target)
        .await;
    assert!(matches!(
        result,
        Err(TargetConstructionError::Reasoning(
            ReasoningError::LimitExceeded(Limit::Matches)
        ))
    ));
    f.store.inner.fail_cursor = true;
    let result = Rdf12TargetConstructor::new(limits())
        .run(&mut f.store, &f.access, &target)
        .await;
    assert!(matches!(
        result,
        Err(TargetConstructionError::Reasoning(ReasoningError::Storage(
            _
        )))
    ));
}

#[tokio::test]
async fn malformed_probes_and_noncanonical_dictionary_keys_abort() {
    let (mut f, target) = fixture().await;
    f.store.inner.mismatched_probe = true;
    let result = Rdf12TargetConstructor::new(limits())
        .run(&mut f.store, &f.access, &target)
        .await;
    assert!(matches!(
        result,
        Err(TargetConstructionError::Reasoning(
            ReasoningError::InvalidProbe
        ))
    ));
    f.store.inner.mismatched_probe = false;
    f.corrupt_quotation_identity();
    let result = Rdf12TargetConstructor::new(limits())
        .run(&mut f.store, &f.access, &target)
        .await;
    assert!(matches!(result, Err(TargetConstructionError::Identity)));
}
