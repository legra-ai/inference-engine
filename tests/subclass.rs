//! Executable two-rule RDFS contract against a bounded test-only store.

use inference_engine::reasoning::{
    ExecutionProfile, Inference, Reasoner, ReasoningStore, RunLimits, TripleCursor, TripleSource,
};
use std::{
    collections::BTreeSet,
    io,
    num::{NonZeroU32, NonZeroU64},
};

const TYPE: u64 = 1;
const SUBCLASS: u64 = 2;
type Triple = [u64; 3];

struct Cursor(std::vec::IntoIter<Triple>);

impl TripleCursor for Cursor {
    type Term = u64;
    type Error = io::Error;
    async fn next(&mut self) -> io::Result<Option<Triple>> {
        Ok(self.0.next())
    }
}

struct Snapshot(BTreeSet<Triple>);

impl TripleSource for Snapshot {
    type Term = u64;
    type Error = io::Error;
    type Cursor = Cursor;
    async fn scan(&self, pattern: [Option<u64>; 3]) -> io::Result<Cursor> {
        // bounded: these test fixtures contain fewer than thirty triples.
        Ok(Cursor(
            self.0
                .iter()
                .filter(|triple| {
                    pattern
                        .iter()
                        .zip(triple.iter())
                        .all(|(p, t)| p.is_none_or(|p| p == *t))
                })
                .copied()
                .collect::<Vec<_>>()
                .into_iter(),
        ))
    }
}

#[derive(Default)]
struct Store {
    asserted: BTreeSet<Triple>,
    inferred: BTreeSet<Triple>,
    evidence: Vec<Inference<u64>>,
    cancelled: bool,
    fail_read: bool,
    fail_write: bool,
}

impl Store {
    fn new(asserted: impl IntoIterator<Item = Triple>) -> Self {
        // bounded: these test fixtures contain fewer than thirty triples.
        Self {
            asserted: asserted.into_iter().collect(),
            ..Self::default()
        }
    }
}

impl ReasoningStore for Store {
    type Term = u64;
    type Error = io::Error;
    type Snapshot = Snapshot;
    async fn resolve_iri(&mut self, iri: &str) -> io::Result<u64> {
        match iri {
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" => Ok(TYPE),
            "http://www.w3.org/2000/01/rdf-schema#subClassOf" => Ok(SUBCLASS),
            _ => Err(io::Error::other("unexpected rule constant")),
        }
    }
    async fn snapshot(&mut self) -> io::Result<Snapshot> {
        if self.fail_read {
            return Err(io::Error::other("read failed"));
        }
        // bounded: these test fixtures contain fewer than thirty triples.
        Ok(Snapshot(
            self.asserted.union(&self.inferred).copied().collect(),
        ))
    }
    async fn contains(&mut self, triple: &Triple) -> io::Result<bool> {
        Ok(self.asserted.contains(triple) || self.inferred.contains(triple))
    }
    async fn insert_inferred(&mut self, inference: Inference<u64>) -> io::Result<bool> {
        if self.fail_write {
            return Err(io::Error::other("write failed"));
        }
        if self.asserted.contains(&inference.triple) {
            return Ok(false);
        }
        let inserted = self.inferred.insert(inference.triple);
        if inserted {
            // bounded: these test fixtures contain fewer than thirty consequences.
            self.evidence.push(inference);
        }
        Ok(inserted)
    }
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

fn limits(inferred: u64, rounds: u32, matches: u64) -> RunLimits {
    RunLimits::new(
        NonZeroU32::new(rounds).unwrap(),
        NonZeroU64::new(inferred).unwrap(),
        NonZeroU64::new(matches).unwrap(),
    )
}

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
        evidence
            .premises
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
async fn never_reverses_subclass_or_reads_an_unselected_graph() {
    let mut store = Store::new([[10, SUBCLASS, 11], [20, TYPE, 11]]);
    reasoner().run(&mut store).await.unwrap();
    assert!(store.inferred.is_empty());
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
    for name in ["rdfs-core", "rdfs", "owl2-rl", "isSubclassOf"] {
        assert!(name.parse::<ExecutionProfile>().is_err());
    }
    assert_eq!(
        ExecutionProfile::RdfsSubclass.rules(),
        &[rdf_reasoning::RuleId::Rdfs9, rdf_reasoning::RuleId::Rdfs11]
    );
}
