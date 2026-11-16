//! Capped, test-only graph adapter; never used by production execution.

use inference_engine::reasoning::{
    Inference, Rdf12TripleTermStore, Rdf12WitnessStore, ReasoningStore, RunLimits, TripleCursor,
    TripleSource,
};
use rdf_reasoning::Rdf12ExistentialAxiomId;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    num::{NonZeroU32, NonZeroU64},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub const TYPE: u64 = 1;
pub const SUBCLASS: u64 = 2;
type Triple = [u64; 3];

pub struct Cursor(std::vec::IntoIter<Triple>, bool);

impl TripleCursor for Cursor {
    type Term = u64;
    type Error = io::Error;
    async fn next(&mut self) -> io::Result<Option<Triple>> {
        let next = self.0.next();
        if next.is_none() && self.1 {
            return Err(io::Error::other("cursor failed after rows"));
        }
        Ok(next)
    }
}

pub struct Snapshot(BTreeSet<Triple>, bool);

impl TripleSource for Snapshot {
    type Term = u64;
    type Error = io::Error;
    type Cursor = Cursor;
    async fn scan(&self, pattern: [Option<u64>; 3]) -> io::Result<Cursor> {
        // bounded: these test fixtures contain at most 4096 tuples, enforced at insertion.
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
            self.1,
        ))
    }
}

#[derive(Default)]
pub struct Store {
    pub asserted: BTreeSet<Triple>,
    pub inferred: BTreeSet<Triple>,
    pub evidence: Vec<Inference<u64>>,
    pub cancelled: bool,
    pub cancel_signal: Option<Arc<AtomicBool>>,
    pub fail_read: bool,
    pub fail_write: bool,
    pub fail_cursor: bool,
    pub fail_after_writes: Option<usize>,
    // bounded: at most four standard existential descriptors per capped test attempt.
    pub witnesses: BTreeMap<Rdf12ExistentialAxiomId, u64>,
    pub witness_scope: u64,
    // bounded: at most 128 term constructions/depth entries per constructor fixture.
    pub constructed: BTreeMap<Triple, u64>,
    pub term_depths: BTreeMap<u64, u32>,
    pub snapshot_calls: u64,
    iris: BTreeMap<String, u64>,
}

impl Store {
    pub fn iri(&mut self, iri: &str) -> u64 {
        match iri {
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" => return TYPE,
            "http://www.w3.org/2000/01/rdf-schema#subClassOf" => return SUBCLASS,
            _ => {}
        }
        if let Some(value) = self.iris.get(iri) {
            return *value;
        }
        assert!(self.iris.len() < 256, "bounded test vocabulary exhausted");
        let value = 1000 + self.iris.len() as u64;
        self.iris.insert(iri.to_owned(), value);
        value
    }
    pub fn new(asserted: impl IntoIterator<Item = Triple>) -> Self {
        // bounded: these test fixtures contain at most 4096 tuples, enforced at insertion.
        let mut store = Self::default();
        for triple in asserted {
            assert!(store.asserted.len() < 4096);
            store.asserted.insert(triple);
        }
        store
    }
}

impl ReasoningStore for Store {
    type Term = u64;
    type Error = io::Error;
    type Snapshot = Snapshot;
    async fn resolve_iri(&mut self, iri: &str) -> io::Result<u64> {
        Ok(self.iri(iri))
    }
    async fn snapshot(&mut self) -> io::Result<Snapshot> {
        self.snapshot_calls += 1;
        if self.fail_read {
            return Err(io::Error::other("read failed"));
        }
        // bounded: these test fixtures contain at most 4096 tuples, enforced at insertion.
        Ok(Snapshot(
            self.asserted.union(&self.inferred).copied().collect(),
            self.fail_cursor,
        ))
    }
    async fn contains(&mut self, triple: &Triple) -> io::Result<bool> {
        Ok(self.asserted.contains(triple) || self.inferred.contains(triple))
    }
    async fn insert_inferred(&mut self, inference: Inference<u64>) -> io::Result<bool> {
        if self.fail_write || self.fail_after_writes == Some(self.evidence.len()) {
            return Err(io::Error::other("write failed"));
        }
        if self.asserted.contains(&inference.triple) {
            return Ok(false);
        }
        assert!(self.asserted.len() + self.inferred.len() < 4096);
        let inserted = self.inferred.insert(inference.triple);
        if inserted {
            // bounded: these test fixtures contain at most 4096 consequences, enforced before insertion.
            self.evidence.push(inference);
        }
        Ok(inserted)
    }
    fn is_cancelled(&self) -> bool {
        self.cancelled
            || self
                .cancel_signal
                .as_ref()
                .is_some_and(|signal| signal.load(Ordering::Relaxed))
    }
}

pub fn limits(inferred: u64, rounds: u32, matches: u64) -> RunLimits {
    RunLimits::new(
        NonZeroU32::new(rounds).unwrap(),
        NonZeroU64::new(inferred).unwrap(),
        NonZeroU64::new(matches).unwrap(),
    )
}

impl Rdf12WitnessStore for Store {
    async fn resolve_existential_blank(&mut self, id: Rdf12ExistentialAxiomId) -> io::Result<u64> {
        if let Some(key) = self.witnesses.get(&id) {
            return Ok(*key);
        }
        assert!(self.witness_scope < 16, "bounded fixture scope exhausted");
        let index = Rdf12ExistentialAxiomId::ALL
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap() as u64;
        let blank = 100 + 4 * self.witness_scope + index;
        assert!(self.witnesses.len() < 4);
        self.witnesses.insert(id, blank);
        Ok(blank)
    }
}

impl Rdf12TripleTermStore for Store {
    async fn resolve_triple_term(&mut self, triple: Triple) -> io::Result<u64> {
        if let Some(key) = self.constructed.get(&triple) {
            return Ok(*key);
        }
        let depth = triple
            .iter()
            .map(|key| self.term_depths.get(key).copied().unwrap_or(0))
            .max()
            .unwrap()
            + 1;
        if depth > 4 {
            return Err(io::Error::other(
                "substituted triple-term nesting limit exceeded",
            ));
        }
        assert!(self.constructed.len() < 128);
        assert!(self.term_depths.len() < 128);
        let key = 500 + self.constructed.len() as u64;
        self.constructed.insert(triple, key);
        self.term_depths.insert(key, depth);
        Ok(key)
    }
}
