//! Recursive occurrence traversal preserves root/path/role without asserting contents.
#[allow(dead_code)]
mod support;
use inference_engine::{
    entailment::EntailmentTerms,
    reasoning::{
        LiteralRole, OccurrenceError, OccurrenceLimits, Rdf12OccurrenceCursor, ReasoningStore,
        TermPosition as P, TripleSource,
    },
};
use std::{
    collections::BTreeMap,
    future::Future,
    io,
    num::{NonZeroU32, NonZeroU64},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
};
use support::Store;

#[derive(Default)]
struct Terms {
    quoted: BTreeMap<u64, [u64; 3]>,
    missing: Option<u64>,
    cancelled: bool,
    signal: Arc<AtomicBool>,
    cache: Mutex<Option<(u64, [u64; 3])>>,
    loads: AtomicUsize,
}
impl EntailmentTerms for Terms {
    type Term = u64;
    type Error = io::Error;
    async fn is_blank(&self, _: &u64) -> io::Result<bool> {
        Ok(false)
    }
    async fn triple_term(&self, key: &u64) -> io::Result<Option<[u64; 3]>> {
        if self.missing == Some(*key) {
            return Err(io::Error::other("authoritative term load failed"));
        }
        if let Some((cached, triple)) = *self.cache.lock().unwrap()
            && cached == *key
        {
            return Ok(Some(triple));
        }
        let value = self.quoted.get(key).copied();
        if let Some(triple) = value {
            self.loads.fetch_add(1, Ordering::Relaxed);
            *self.cache.lock().unwrap() = Some((*key, triple));
        }
        Ok(value)
    }
    fn is_cancelled(&self) -> bool {
        self.cancelled || self.signal.load(Ordering::Relaxed)
    }
}
fn limits(depth: u32, occurrences: u64, steps: u64) -> OccurrenceLimits {
    OccurrenceLimits::new(
        NonZeroU32::new(depth).unwrap(),
        NonZeroU64::new(occurrences).unwrap(),
        NonZeroU64::new(steps).unwrap(),
    )
}
async fn roots(rows: &[[u64; 3]], late: bool) -> support::Cursor {
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
async fn visits_nested_terms_and_predicates_with_exact_path_and_role() {
    let terms = Terms {
        quoted: BTreeMap::from([(90, [3, 4, 91]), (91, [5, 6, 7])]),
        ..Terms::default()
    };
    for role in [LiteralRole::Source, LiteralRole::TargetContext] {
        let mut cursor = Rdf12OccurrenceCursor::new(
            roots(&[[1, 2, 90]], false).await,
            &terms,
            role,
            limits(3, 9, 30),
        );
        let expected = [
            (1, vec![P::Subject]),
            (2, vec![P::Predicate]),
            (90, vec![P::Object]),
            (3, vec![P::Object, P::Subject]),
            (4, vec![P::Object, P::Predicate]),
            (91, vec![P::Object, P::Object]),
            (5, vec![P::Object, P::Object, P::Subject]),
            (6, vec![P::Object, P::Object, P::Predicate]),
            (7, vec![P::Object, P::Object, P::Object]),
        ];
        for (term, path) in expected {
            let occurrence = cursor.next().await.unwrap().unwrap();
            assert_eq!(occurrence.term, term);
            assert_eq!(
                occurrence.path.positions().copied().collect::<Vec<_>>(),
                path
            );
            assert_eq!(occurrence.role, role);
            assert_eq!(occurrence.root, [1, 2, 90]);
            assert_eq!(
                occurrence.is_predicate(),
                occurrence.path.positions().next_back() == Some(&P::Predicate)
            );
            let json = serde_json::to_string(&occurrence).unwrap();
            assert_eq!(
                serde_json::from_str::<inference_engine::reasoning::TermOccurrence<u64>>(&json)
                    .unwrap(),
                occurrence
            );
            assert_eq!(
                postcard::from_bytes::<inference_engine::reasoning::TermOccurrence<u64>>(
                    &postcard::to_allocvec(&occurrence).unwrap()
                )
                .unwrap(),
                occurrence
            );
        }
        assert!(cursor.next().await.unwrap().is_none());
        assert!(cursor.next().await.unwrap().is_none());
    }
}
#[tokio::test]
async fn repeated_keys_keep_distinct_paths_and_roots() {
    let terms = Terms {
        quoted: BTreeMap::from([(90, [1, 2, 1])]),
        ..Terms::default()
    };
    let mut cursor = Rdf12OccurrenceCursor::new(
        roots(&[[1, 2, 90], [3, 2, 90]], false).await,
        &terms,
        LiteralRole::Source,
        limits(2, 12, 30),
    );
    let mut count = 0;
    let mut ones = 0;
    while let Some(occurrence) = cursor.next().await.unwrap() {
        count += 1;
        if occurrence.term == 1 {
            ones += 1;
        }
    }
    assert_eq!(count, 12);
    assert_eq!(ones, 5);
}
#[tokio::test]
async fn authoritative_load_failures_do_not_become_non_triple_terms() {
    let terms = Terms {
        missing: Some(90),
        ..Terms::default()
    };
    let mut cursor = Rdf12OccurrenceCursor::new(
        roots(&[[1, 2, 90]], false).await,
        &terms,
        LiteralRole::Source,
        limits(4, 30, 50),
    );
    cursor.next().await.unwrap();
    cursor.next().await.unwrap();
    assert!(matches!(
        cursor.next().await,
        Err(OccurrenceError::Storage(_))
    ));
}
#[tokio::test]
async fn depth_cycles_occurrence_and_work_budgets_are_hard_errors() {
    let terms = Terms {
        quoted: BTreeMap::from([(90, [1, 2, 90])]),
        ..Terms::default()
    };
    for (budget, kind) in [
        (limits(2, 30, 100), 0),
        (limits(10, 1, 100), 1),
        (limits(10, 30, 1), 2),
    ] {
        let mut cursor = Rdf12OccurrenceCursor::new(
            roots(&[[1, 2, 90]], false).await,
            &terms,
            LiteralRole::Source,
            budget,
        );
        let error = loop {
            match cursor.next().await {
                Ok(Some(_)) => {}
                Ok(None) => panic!("silently truncated"),
                Err(e) => break e,
            }
        };
        assert!(matches!(
            (kind, error),
            (0, OccurrenceError::DepthLimit)
                | (1, OccurrenceError::OccurrenceLimit)
                | (2, OccurrenceError::StepLimit)
        ));
    }
}
#[tokio::test]
async fn cancellation_and_late_root_errors_invalidate_partial_traversal() {
    let terms = Terms::default();
    let mut cursor = Rdf12OccurrenceCursor::new(
        roots(&[[1, 2, 3]], true).await,
        &terms,
        LiteralRole::Source,
        limits(1, 3, 10),
    );
    for _ in 0..3 {
        cursor.next().await.unwrap();
    }
    assert!(matches!(
        cursor.next().await,
        Err(OccurrenceError::Storage(_))
    ));
    let cancelled = Terms {
        cancelled: true,
        ..Terms::default()
    };
    let mut cursor = Rdf12OccurrenceCursor::new(
        roots(&[], false).await,
        &cancelled,
        LiteralRole::Source,
        limits(1, 3, 10),
    );
    assert!(matches!(
        cursor.next().await,
        Err(OccurrenceError::Cancelled)
    ));
}

#[tokio::test]
async fn evicted_nested_structure_is_reloaded_instead_of_becoming_an_atomic_term() {
    let terms = Terms {
        quoted: BTreeMap::from([(90, [5, 6, 7]), (91, [8, 9, 10])]),
        ..Terms::default()
    };
    let mut cursor = Rdf12OccurrenceCursor::new(
        roots(&[[1, 2, 90], [3, 2, 91], [4, 2, 90]], false).await,
        &terms,
        LiteralRole::Source,
        limits(2, 18, 30),
    );
    let mut count = 0;
    while cursor.next().await.unwrap().is_some() {
        count += 1;
    }
    assert_eq!(count, 18);
    assert_eq!(terms.loads.load(Ordering::Relaxed), 3);
}
#[tokio::test]
async fn cached_ready_traversal_yields_and_observes_cancellation() {
    let terms = Terms::default();
    // bounded: exactly 1000 source fixture rows, enforced by Store's 4096 cap.
    let mut store = Store::new((10..1010).map(|member| [member, 2, 3]));
    let roots = store
        .snapshot()
        .await
        .unwrap()
        .scan([None; 3])
        .await
        .unwrap();
    let mut cursor =
        Rdf12OccurrenceCursor::new(roots, &terms, LiteralRole::Source, limits(1, 3000, 5000));
    let future = async {
        while cursor.next().await?.is_some() {}
        Ok::<(), OccurrenceError<io::Error>>(())
    };
    let mut future = Box::pin(future);
    let mut context = Context::from_waker(Waker::noop());
    assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
    terms.signal.store(true, Ordering::Relaxed);
    assert!(matches!(future.await, Err(OccurrenceError::Cancelled)));
}
