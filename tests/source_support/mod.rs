//! Bounded memory-only source adapter; production stores supply spilled scratch.
#[allow(dead_code)]
#[path = "../support/mod.rs"]
mod support;
use inference_engine::{
    entailment::EntailmentTerms,
    reasoning::{
        DatatypeConflict, DatatypeConflictSink, Inference, LiteralReader, LiteralTermAccess,
        Rdf12ContainerAccess, Rdf12TripleTermStore, Rdf12WitnessStore, ReasoningStore,
    },
};
use rdf_reasoning::{Rdf12ContainerProperty, Rdf12ExistentialAxiomId};
use rdf_structural_model::{Literal, LiteralMetadata};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    num::NonZeroU32,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Default)]
struct Dictionary {
    // bounded: at most 256 fixture/rule IRIs, checked before insert.
    iris: BTreeMap<u64, Arc<str>>,
    // bounded: at most 1024 fixture constructor entries, enforced by inner Store.
    quoted: BTreeMap<u64, [u64; 3]>,
    // bounded: four standard nonempty descriptors per source attempt.
    blanks: BTreeSet<u64>,
    // bounded: at most 16 literal records of at most 32 KiB each.
    literals: BTreeMap<u64, Arc<LiteralData>>,
    wrong_container: bool,
    wrong_quotation: bool,
    cancel: Arc<AtomicBool>,
}
impl Dictionary {
    fn known(&self, key: &u64) -> bool {
        self.iris.contains_key(key)
            || self.quoted.contains_key(key)
            || self.blanks.contains(key)
            || self.literals.contains_key(key)
    }
    fn iri(&mut self, key: u64, iri: &str) {
        assert!(self.iris.contains_key(&key) || self.iris.len() < 256);
        assert!(iri.len() <= 4096); // bounded: static rule/fixture vocabulary only.
        self.iris.insert(key, Arc::from(iri));
    }
}
pub struct Store {
    pub inner: support::Store,
    dictionary: Arc<Mutex<Dictionary>>,
}
pub struct Access(Arc<Mutex<Dictionary>>);
pub struct Fixture {
    pub store: Store,
    pub access: Access,
}
impl Fixture {
    pub fn new() -> Self {
        let dictionary = Arc::new(Mutex::new(Dictionary::default()));
        let mut inner = support::Store::new([]);
        inner.cancel_signal = Some(dictionary.lock().unwrap().cancel.clone());
        Self {
            store: Store {
                inner,
                dictionary: dictionary.clone(),
            },
            access: Access(dictionary),
        }
    }
    pub fn corrupt_quotation_identity(&mut self) {
        self.store.dictionary.lock().unwrap().wrong_quotation = true;
    }
    pub fn corrupt_container_classification(&mut self) {
        self.store.dictionary.lock().unwrap().wrong_container = true;
    }
    pub fn iri(&mut self, iri: &str) -> u64 {
        let key = self.store.inner.iri(iri);
        self.store.dictionary.lock().unwrap().iri(key, iri);
        key
    }
    pub fn literal(&mut self, payload: &str) -> u64 {
        assert!(payload.len() <= 32768);
        let mut dictionary = self.store.dictionary.lock().unwrap();
        assert!(dictionary.literals.len() < 16);
        let key = 500 + dictionary.literals.len() as u64;
        dictionary.literals.insert(
            key,
            Arc::new(LiteralData {
                metadata: Literal::string("").metadata().clone(),
                payload: payload.as_bytes().to_vec(),
            }),
        );
        key
    }
    pub async fn quoted(&mut self, triple: [u64; 3]) -> u64 {
        self.store.resolve_triple_term(triple).await.unwrap()
    }
}
impl ReasoningStore for Store {
    type Term = u64;
    type Error = io::Error;
    type Snapshot = <support::Store as ReasoningStore>::Snapshot;
    async fn resolve_iri(&mut self, iri: &str) -> io::Result<u64> {
        let key = self.inner.resolve_iri(iri).await?;
        self.dictionary.lock().unwrap().iri(key, iri);
        Ok(key)
    }
    async fn snapshot(&mut self) -> io::Result<Self::Snapshot> {
        self.inner.snapshot().await
    }
    async fn contains(&mut self, triple: &[u64; 3]) -> io::Result<bool> {
        self.inner.contains(triple).await
    }
    async fn insert_inferred(&mut self, value: Inference<u64>) -> io::Result<bool> {
        self.inner.insert_inferred(value).await
    }
    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }
}
impl Rdf12WitnessStore for Store {
    async fn resolve_existential_blank(&mut self, id: Rdf12ExistentialAxiomId) -> io::Result<u64> {
        let key = self.inner.resolve_existential_blank(id).await?;
        let mut dictionary = self.dictionary.lock().unwrap();
        assert!(dictionary.blanks.len() < 4 || dictionary.blanks.contains(&key));
        dictionary.blanks.insert(key);
        Ok(key)
    }
}
impl Rdf12TripleTermStore for Store {
    async fn resolve_triple_term(&mut self, triple: [u64; 3]) -> io::Result<u64> {
        let key = self.inner.resolve_triple_term(triple).await?;
        let mut dictionary = self.dictionary.lock().unwrap();
        let key = if dictionary.wrong_quotation {
            key + 50_000
        } else {
            key
        };
        dictionary.quoted.insert(key, triple);
        Ok(key)
    }
}
impl EntailmentTerms for Access {
    type Term = u64;
    type Error = io::Error;
    async fn is_blank(&self, key: &u64) -> io::Result<bool> {
        let dictionary = self.0.lock().unwrap();
        if !dictionary.known(key) {
            return Err(io::Error::other("unknown fixture dictionary term"));
        }
        Ok(dictionary.blanks.contains(key))
    }
    async fn triple_term(&self, key: &u64) -> io::Result<Option<[u64; 3]>> {
        let dictionary = self.0.lock().unwrap();
        if !dictionary.known(key) {
            return Err(io::Error::other("unknown fixture dictionary term"));
        }
        Ok(dictionary.quoted.get(key).copied())
    }
    fn is_cancelled(&self) -> bool {
        self.0.lock().unwrap().cancel.load(Ordering::Relaxed)
    }
}
struct LiteralData {
    metadata: LiteralMetadata,
    payload: Vec<u8>,
}
pub struct Reader {
    data: Arc<LiteralData>,
    offset: usize,
    cancel: Arc<AtomicBool>,
}
impl LiteralReader for Reader {
    type Error = io::Error;
    fn metadata(&self) -> LiteralMetadata<&str> {
        self.data.metadata.as_ref()
    }
    async fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let size = out.len().min(self.data.payload.len() - self.offset);
        out[..size].copy_from_slice(&self.data.payload[self.offset..self.offset + size]);
        self.offset += size;
        Ok(size)
    }
    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}
impl LiteralTermAccess for Access {
    type Reader<'a> = Reader;
    async fn open_literal<'a>(&'a self, key: &u64) -> io::Result<Option<Self::Reader<'a>>> {
        let dictionary = self.0.lock().unwrap();
        if !dictionary.known(key) {
            return Err(io::Error::other("unknown fixture dictionary term"));
        }
        Ok(dictionary.literals.get(key).map(|data| Reader {
            data: data.clone(),
            offset: 0,
            cancel: dictionary.cancel.clone(),
        }))
    }
}
impl Rdf12ContainerAccess for Access {
    type ContainerIri<'a> = Arc<str>;
    async fn container_property<'a>(
        &'a self,
        key: &u64,
        maximum: NonZeroU32,
    ) -> io::Result<Option<Rdf12ContainerProperty<Self::ContainerIri<'a>>>> {
        let dictionary = self.0.lock().unwrap();
        if !dictionary.known(key) {
            return Err(io::Error::other("unknown fixture dictionary term"));
        }
        let Some(iri) = dictionary.iris.get(key) else {
            return Ok(None);
        };
        if !iri.starts_with("http://www.w3.org/1999/02/22-rdf-syntax-ns#_") {
            return Ok(None);
        }
        if iri.len() > maximum.get() as usize {
            return Err(io::Error::other("container IRI metadata budget exceeded"));
        }
        if dictionary.wrong_container {
            return Ok(Some(
                Rdf12ContainerProperty::try_new(Arc::<str>::from(
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#_1",
                ))
                .unwrap(),
            ));
        }
        Ok(Rdf12ContainerProperty::try_new(iri.clone()).ok())
    }
}
#[derive(Default)]
pub struct Diagnostics(pub Vec<DatatypeConflict<u64>>, pub bool);
impl DatatypeConflictSink for Diagnostics {
    type Term = u64;
    type Error = io::Error;
    async fn record(&mut self, value: DatatypeConflict<u64>) -> io::Result<()> {
        if self.1 {
            return Err(io::Error::other("diagnostic sink failed"));
        }
        assert!(self.0.len() < 100); // bounded: source fixture diagnostics explicitly capped at 100.
        self.0.push(value);
        Ok(())
    }
}
