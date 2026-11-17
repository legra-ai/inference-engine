//! Cached-ready backends must not monopolize one poll until an entire large run ends.
#[allow(
    dead_code,
    reason = "shared capped adapter supports all reasoning fixtures"
)]
mod support;
use inference_engine::{
    entailment::{EntailmentError, EntailmentLimits, EntailmentTerms, SimpleEntailment},
    reasoning::{
        ExecutionProfile, LiteralOccurrence, LiteralOccurrenceCursor, LiteralReader, LiteralRole,
        LiteralTypingLimits, Rdf12LiteralTyper, Rdf12LiteralValidator, Reasoner, ReasoningError,
        ReasoningStore,
    },
};
use rdf_structural_model::{Literal, LiteralMetadata};
use std::{
    future::Future,
    io,
    num::{NonZeroU32, NonZeroU64},
    pin::pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Waker},
};
use support::{SUBCLASS, Store, TYPE, limits};

async fn cancel_after_one_poll<F: Future>(future: F, signal: &AtomicBool) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    assert!(
        future.as_mut().poll(&mut context).is_pending(),
        "a large cached-ready run completed without yielding"
    );
    signal.store(true, Ordering::Relaxed);
    future.await
}

#[tokio::test]
async fn cached_graph_reasoning_yields_before_completing_the_whole_closure() {
    let signal = Arc::new(AtomicBool::new(false));
    // bounded: 1001 assertions, and fewer than 1000 additional facts before cancellation.
    let mut store = Store::new(
        (0..1000)
            .map(|n| [10_000 + n, TYPE, 10])
            .chain([[10, SUBCLASS, 11]]),
    );
    store.cancel_signal = Some(signal.clone());
    let reasoner = Reasoner::new(ExecutionProfile::RdfsSubclass, limits(2000, 10, 100_000));
    assert!(matches!(
        cancel_after_one_poll(reasoner.run(&mut store), &signal).await,
        Err(ReasoningError::Cancelled)
    ));
    assert!(store.inferred.len() < 1000);
}

struct Reader {
    metadata: LiteralMetadata,
    remaining: u64,
    signal: Arc<AtomicBool>,
}
impl LiteralReader for Reader {
    type Error = io::Error;
    fn metadata(&self) -> LiteralMetadata<&str> {
        self.metadata.as_ref()
    }
    fn is_cancelled(&self) -> bool {
        self.signal.load(Ordering::Relaxed)
    }
    async fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = output.len().min(self.remaining as usize);
        output[..count].fill(b'a');
        self.remaining -= count as u64;
        Ok(count)
    }
}
#[tokio::test]
async fn cached_lexical_stream_yields_without_hydrating_the_whole_value() {
    let signal = Arc::new(AtomicBool::new(false));
    let mut reader = Reader {
        metadata: Literal::string("").metadata().clone(),
        remaining: 5_000_000,
        signal: signal.clone(),
    };
    let validator = Rdf12LiteralValidator::new(5_000_000);
    assert!(
        cancel_after_one_poll(validator.validate(&mut reader), &signal)
            .await
            .is_err()
    );
    assert!(reader.remaining > 0);
}

struct Occurrences {
    remaining: u64,
    signal: Arc<AtomicBool>,
}
impl LiteralOccurrenceCursor for Occurrences {
    type Term = u64;
    type Error = io::Error;
    type Reader = Reader;
    async fn next(
        &mut self,
    ) -> inference_engine::reasoning::LiteralOccurrenceRead<u64, Reader, io::Error> {
        if self.remaining == 0 {
            return Ok(None);
        }
        self.remaining -= 1;
        Ok(Some((
            LiteralOccurrence {
                path: inference_engine::reasoning::OccurrencePath::root(
                    inference_engine::reasoning::TermPosition::Object,
                ),
                literal: 50,
                root: [10, 11, 50],
                role: LiteralRole::Source,
            },
            Reader {
                metadata: Literal::string("").metadata().clone(),
                remaining: 0,
                signal: self.signal.clone(),
            },
        )))
    }
}
#[tokio::test]
async fn many_empty_cached_literals_yield_even_with_zero_total_byte_budget() {
    let signal = Arc::new(AtomicBool::new(false));
    let mut store = Store::new([]);
    store.cancel_signal = Some(signal.clone());
    let mut cursor = Occurrences {
        remaining: 1000,
        signal: signal.clone(),
    };
    let typer = Rdf12LiteralTyper::new(
        LiteralTypingLimits::new(
            NonZeroU64::new(1000).unwrap(),
            NonZeroU64::new(1).unwrap(),
            0,
        ),
        LiteralRole::Source,
    );
    assert!(
        cancel_after_one_poll(typer.run(&mut store, &mut cursor), &signal)
            .await
            .is_err()
    );
    assert!(cursor.remaining > 0);
}

struct Terms(Arc<AtomicBool>);
impl EntailmentTerms for Terms {
    type Term = u64;
    type Error = io::Error;
    async fn is_blank(&self, term: &u64) -> io::Result<bool> {
        Ok((100..200).contains(term))
    }
    async fn triple_term(&self, _: &u64) -> io::Result<Option<[u64; 3]>> {
        Ok(None)
    }
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
#[tokio::test]
async fn cached_entailment_backtracking_yields_before_exhausting_all_candidates() {
    let signal = Arc::new(AtomicBool::new(false));
    // bounded: 1000 fixture source rows and two target patterns.
    let source = Store::new((0..1000).map(|n| [10_000 + n, 1, 20]))
        .snapshot()
        .await
        .unwrap();
    let target = Store::new([[100, 1, 101], [100, 3, 20]])
        .snapshot()
        .await
        .unwrap();
    let terms = Terms(signal.clone());
    let checker = SimpleEntailment::new(EntailmentLimits::new(
        NonZeroU32::new(2).unwrap(),
        NonZeroU32::new(2).unwrap(),
        NonZeroU32::new(3).unwrap(),
        NonZeroU64::new(100_000).unwrap(),
    ));
    assert!(matches!(
        cancel_after_one_poll(checker.check(&source, &target, &terms), &signal).await,
        Err(EntailmentError::Cancelled)
    ));
}
