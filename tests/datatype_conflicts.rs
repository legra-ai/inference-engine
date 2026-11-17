//! Source-only default-map conflicts are diagnostics, never RDF output.
mod support;
use inference_engine::reasoning::{
    DatatypeConflict, DatatypeConflictError, DatatypeConflictLimits, DatatypeConflictSink,
    ExecutionProfile, LiteralRole, Rdf12AxiomSeeder, Rdf12DatatypeConflictChecker, Reasoner,
};
use rdf_reasoning::{Rdf12DatatypeConflictId as C, Rdf12ExistentialAxiomId as E};
use std::{
    future::Future,
    io,
    num::NonZeroU64,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
};
use support::{SUBCLASS, Store, TYPE, limits};

#[derive(Default)]
struct Sink {
    values: Vec<DatatypeConflict<u64>>,
    fail: bool,
}
impl DatatypeConflictSink for Sink {
    type Term = u64;
    type Error = io::Error;
    async fn record(&mut self, value: DatatypeConflict<u64>) -> io::Result<()> {
        if self.fail {
            return Err(io::Error::other("diagnostic write failed"));
        }
        assert!(self.values.len() < 8); // bounded: fixtures emit at most three pairs.
        self.values.push(value);
        Ok(())
    }
}
fn checker(diagnostics: u64, matches: u64) -> Rdf12DatatypeConflictChecker {
    Rdf12DatatypeConflictChecker::new(DatatypeConflictLimits::new(
        NonZeroU64::new(diagnostics).unwrap(),
        NonZeroU64::new(matches).unwrap(),
    ))
}
fn pair(store: &mut Store, id: C, member: u64) -> [[u64; 3]; 2] {
    id.datatypes().map(|d| [member, TYPE, store.iri(d.iri())])
}

#[tokio::test]
async fn each_shared_pair_retains_exact_evidence_without_writing_rdf() {
    for id in C::ALL {
        let mut store = Store::new([]);
        let premises = pair(&mut store, *id, 7);
        store.asserted.extend(premises);
        let mut sink = Sink::default();
        let report = checker(1, 20)
            .run(&mut store, LiteralRole::Source, &mut sink)
            .await
            .unwrap();
        assert_eq!(report.diagnostics, 1);
        assert_eq!(store.snapshot_calls, 1);
        assert!(store.inferred.is_empty());
        assert!(store.evidence.is_empty());
        assert_eq!(
            sink.values,
            [DatatypeConflict {
                id: *id,
                member: 7,
                premises
            }]
        );
        let value = &sink.values[0];
        assert_eq!(
            &serde_json::from_str::<DatatypeConflict<u64>>(&serde_json::to_string(value).unwrap())
                .unwrap(),
            value
        );
        assert_eq!(
            &postcard::from_bytes::<DatatypeConflict<u64>>(&postcard::to_allocvec(value).unwrap())
                .unwrap(),
            value
        );
    }
}
#[tokio::test]
async fn missing_different_and_opaque_memberships_are_not_conflicts() {
    let mut store = Store::new([]);
    let [left, mut right] = pair(&mut store, C::StringLangString, 7);
    right[0] = 8;
    store
        .asserted
        .extend([left, right, [7, TYPE, 90], [7, 90, right[2]]]);
    let mut sink = Sink::default();
    assert_eq!(
        checker(1, 30)
            .run(&mut store, LiteralRole::Source, &mut sink)
            .await
            .unwrap()
            .diagnostics,
        0
    );
    assert!(sink.values.is_empty());
}
#[tokio::test]
async fn scoped_nonempty_witness_detects_a_subclass_clash_without_literal_data() {
    for id in C::ALL {
        let mut store = Store::new([]);
        let [a, b] = id.datatypes();
        let left = store.iri(a.iri());
        let right = store.iri(b.iri());
        store.asserted.insert([left, SUBCLASS, right]);
        Rdf12AxiomSeeder::new(NonZeroU64::new(1).unwrap())
            .seed_existential(&mut store, E::RdfD1a(a))
            .await
            .unwrap();
        Reasoner::new(ExecutionProfile::RdfsSubclass, limits(10, 5, 100))
            .run(&mut store)
            .await
            .unwrap();
        let mut sink = Sink::default();
        checker(1, 30)
            .run(&mut store, LiteralRole::Source, &mut sink)
            .await
            .unwrap();
        assert_eq!(sink.values.len(), 1);
        assert_eq!(sink.values[0].id, *id);
        assert_eq!(sink.values[0].member, store.witnesses[&E::RdfD1a(a)]);
    }
}
#[tokio::test]
async fn target_context_is_rejected_before_any_read_or_diagnostic() {
    let mut store = Store::new([]);
    store.fail_read = true;
    let mut sink = Sink::default();
    assert!(matches!(
        checker(1, 30)
            .run(&mut store, LiteralRole::TargetContext, &mut sink)
            .await,
        Err(DatatypeConflictError::WrongRole)
    ));
    assert_eq!(store.snapshot_calls, 0);
    assert!(sink.values.is_empty());
}
#[tokio::test]
async fn read_cursor_write_cancel_and_limits_never_return_a_success_report() {
    for failure in 0..6 {
        let mut store = Store::new([]);
        let all = C::ALL
            .iter()
            .flat_map(|id| pair(&mut store, *id, 7))
            .collect::<Vec<_>>(); // bounded: six fixed premises.
        store.asserted.extend(all);
        let mut sink = Sink::default();
        match failure {
            0 => store.fail_read = true,
            1 => store.fail_cursor = true,
            2 => sink.fail = true,
            3 => store.cancelled = true,
            _ => {}
        }
        let result = checker(
            if failure == 4 { 1 } else { 8 },
            if failure == 5 { 1 } else { 50 },
        )
        .run(&mut store, LiteralRole::Source, &mut sink)
        .await;
        assert!(result.is_err(), "failure {failure}");
        assert!(store.inferred.is_empty());
        if failure == 4 {
            assert_eq!(sink.values.len(), 1);
        }
    }
}

#[tokio::test]
async fn cached_ready_source_checks_yield_and_observe_cancellation() {
    let mut store = Store::new([]);
    let left = store.iri(C::StringLangString.datatypes()[0].iri());
    for member in 10..1010 {
        store.asserted.insert([member, TYPE, left]);
    }
    let signal = Arc::new(AtomicBool::new(false));
    store.cancel_signal = Some(signal.clone());
    let mut sink = Sink::default();
    let checker = checker(1, 10000);
    let mut future = Box::pin(checker.run(&mut store, LiteralRole::Source, &mut sink));
    let mut context = Context::from_waker(Waker::noop());
    assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
    signal.store(true, Ordering::Relaxed);
    assert!(matches!(
        future.await,
        Err(DatatypeConflictError::Execution(
            inference_engine::reasoning::ReasoningError::Cancelled
        ))
    ));
    assert!(sink.values.is_empty());
}
