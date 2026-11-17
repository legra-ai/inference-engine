//! Legal target dependencies require construction interleaved with core closure.
#[allow(dead_code)]
mod support;
use inference_engine::{
    entailment::{EntailmentLimits, EntailmentTerms, SimpleEntailment},
    reasoning::{
        ExecutionProfile, LiteralRole, OccurrenceLimits, PropositionLimits, Rdf12OccurrenceCursor,
        Rdf12OccurrenceTyper, Rdf12PropositionConstructor, Reasoner, ReasoningStore, TripleSource,
    },
};
use std::{
    collections::BTreeMap,
    io,
    num::{NonZeroU32, NonZeroU64},
};
use support::{SUBCLASS, Store, TYPE};
struct Terms(BTreeMap<u64, [u64; 3]>);
impl EntailmentTerms for Terms {
    type Term = u64;
    type Error = io::Error;
    async fn is_blank(&self, key: &u64) -> io::Result<bool> {
        Ok((41..44).contains(key))
    }
    async fn triple_term(&self, key: &u64) -> io::Result<Option<[u64; 3]>> {
        Ok(self.0.get(key).copied())
    }
    fn is_cancelled(&self) -> bool {
        false
    }
}
fn n(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap()
}
fn core() -> Reasoner {
    Reasoner::new(
        ExecutionProfile::RdfsCore,
        support::limits(1000, 16, 100_000),
    )
}
async fn matches(store: &mut Store, terms: &Terms, target: &[[u64; 3]]) -> bool {
    let source = store.snapshot().await.unwrap();
    let target = Store::new(target.iter().copied()).snapshot().await.unwrap();
    SimpleEntailment::new(EntailmentLimits::new(
        NonZeroU32::new(8).unwrap(),
        NonZeroU32::new(16).unwrap(),
        NonZeroU32::new(64).unwrap(),
        n(100_000),
    ))
    .check(&source, &target, terms)
    .await
    .unwrap()
    .entailed
}
async fn construct(store: &mut Store, candidate: [u64; 3]) -> u64 {
    let mut candidate_store = Store::new([candidate]);
    let mut cursor = candidate_store
        .snapshot()
        .await
        .unwrap()
        .scan([None; 3])
        .await
        .unwrap();
    Rdf12PropositionConstructor::new(PropositionLimits::new(n(1), n(4)))
        .run_candidates(store, &mut cursor)
        .await
        .unwrap()
        .inferred
}
#[tokio::test]
async fn a_second_goal_depending_on_the_first_constructed_term_needs_the_intervening_core() {
    let mut store = Store::new([[10, 20, 35]]);
    let property = store.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#Property");
    let proposition = store.iri("http://www.w3.org/2000/01/rdf-schema#Proposition");
    let resource = store.iri("http://www.w3.org/2000/01/rdf-schema#Resource");
    let subproperty = store.iri("http://www.w3.org/2000/01/rdf-schema#subPropertyOf");
    store
        .asserted
        .extend([[32, TYPE, property], [proposition, SUBCLASS, property]]);
    let mut terms = Terms(BTreeMap::from([(91, [41, 32, 33]), (92, [42, 32, 34])]));
    let target = [
        [10, 20, 41],
        [42, subproperty, 91],
        [42, TYPE, proposition],
        [43, subproperty, 92],
        [43, TYPE, proposition],
    ];
    let roots = store
        .snapshot()
        .await
        .unwrap()
        .scan([None; 3])
        .await
        .unwrap();
    let mut source = Rdf12OccurrenceCursor::new(
        roots,
        &terms,
        LiteralRole::Source,
        OccurrenceLimits::new(NonZeroU32::new(3).unwrap(), n(100), n(1000)),
    );
    Rdf12OccurrenceTyper::new(n(100), LiteralRole::Source)
        .run(&mut store, &mut source)
        .await
        .unwrap();
    core().run(&mut store).await.unwrap();
    let roots = Store::new(target)
        .snapshot()
        .await
        .unwrap()
        .scan([None; 3])
        .await
        .unwrap();
    let mut context = Rdf12OccurrenceCursor::new(
        roots,
        &terms,
        LiteralRole::TargetContext,
        OccurrenceLimits::new(NonZeroU32::new(3).unwrap(), n(100), n(1000)),
    );
    Rdf12OccurrenceTyper::new(n(100), LiteralRole::TargetContext)
        .run(&mut store, &mut context)
        .await
        .unwrap();
    assert!(!matches(&mut store, &terms, &target).await);
    assert_eq!(construct(&mut store, [35, 32, 33]).await, 1);
    let first = store.constructed[&[35, 32, 33]];
    terms.0.insert(first, [35, 32, 33]);
    assert!(!store.inferred.contains(&[first, TYPE, resource]));
    assert_eq!(construct(&mut store, [first, 32, 34]).await, 0);
    assert!(!matches(&mut store, &terms, &target).await);
    core().run(&mut store).await.unwrap();
    assert!(store.inferred.contains(&[first, TYPE, resource]));
    assert!(store.inferred.contains(&[first, subproperty, first]));
    assert!(!matches(&mut store, &terms, &target).await);
    assert_eq!(construct(&mut store, [first, 32, 34]).await, 1);
    let second = store.constructed[&[first, 32, 34]];
    terms.0.insert(second, [first, 32, 34]);
    assert!(!matches(&mut store, &terms, &target).await);
    core().run(&mut store).await.unwrap();
    assert!(matches(&mut store, &terms, &target).await);
    assert_eq!(store.constructed.len(), 2);
    assert!(!store.inferred.contains(&[35, 32, 33]));
    assert!(!store.inferred.contains(&[first, 32, 34]));
}
