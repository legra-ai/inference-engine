//! Compose actual occurrence typing, finite construction and nested substitution.
#[allow(dead_code)]
mod support;
use inference_engine::{
    entailment::{EntailmentLimits, EntailmentTerms, SimpleEntailment},
    reasoning::{
        LiteralRole, OccurrenceLimits, PropositionLimits, Rdf12OccurrenceCursor,
        Rdf12OccurrenceTyper, Rdf12PropositionConstructor, ReasoningStore, TripleSource,
    },
};
use std::{
    collections::BTreeMap,
    io,
    num::{NonZeroU32, NonZeroU64},
};
use support::{Store, TYPE};

#[derive(Default)]
struct Terms {
    quoted: BTreeMap<u64, [u64; 3]>,
}
impl EntailmentTerms for Terms {
    type Term = u64;
    type Error = io::Error;
    async fn is_blank(&self, key: &u64) -> io::Result<bool> {
        Ok([31, 41, 42].contains(key))
    }
    async fn triple_term(&self, key: &u64) -> io::Result<Option<[u64; 3]>> {
        Ok(self.quoted.get(key).copied())
    }
    fn is_cancelled(&self) -> bool {
        false
    }
}
fn n(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap()
}
fn fixture(nested: bool) -> (Store, Terms, u64) {
    let root = if nested { 92 } else { 90 };
    let mut store = Store::new([[10, 20, root], [31, 21, 34]]);
    store.constructed.insert([31, 32, 33], 90);
    store.term_depths.insert(90, 1);
    if nested {
        store.constructed.insert([90, 22, 31], 92);
        store.term_depths.insert(92, 2);
    }
    let proposition = store.iri("http://www.w3.org/2000/01/rdf-schema#Proposition");
    let terms = Terms {
        quoted: BTreeMap::from([
            (90, [31, 32, 33]),
            (91, [41, 32, 33]),
            (92, [90, 22, 31]),
            (93, [91, 22, 41]),
            (94, [91, 22, 35]),
        ]),
    };
    (store, terms, proposition)
}
async fn match_target(store: &mut Store, terms: &Terms, target: &[[u64; 3]]) -> bool {
    let source = store.snapshot().await.unwrap();
    let target = Store::new(target.iter().copied()).snapshot().await.unwrap();
    SimpleEntailment::new(EntailmentLimits::new(
        NonZeroU32::new(8).unwrap(),
        NonZeroU32::new(32).unwrap(),
        NonZeroU32::new(64).unwrap(),
        n(100_000),
    ))
    .check(&source, &target, terms)
    .await
    .unwrap()
    .entailed
}
async fn type_propositions(store: &mut Store, terms: &mut Terms) {
    let roots = store
        .snapshot()
        .await
        .unwrap()
        .scan([None; 3])
        .await
        .unwrap();
    let mut cursor = Rdf12OccurrenceCursor::new(
        roots,
        terms,
        LiteralRole::Source,
        OccurrenceLimits::new(NonZeroU32::new(4).unwrap(), n(100), n(1000)),
    );
    Rdf12OccurrenceTyper::new(n(100), LiteralRole::Source)
        .run(store, &mut cursor)
        .await
        .unwrap();
    Rdf12PropositionConstructor::new(PropositionLimits::new(n(512), n(10_000)))
        .run(store)
        .await
        .unwrap();
    // bounded: fixture dictionary admits at most 512 constructed terms.
    for (triple, key) in &store.constructed {
        terms.quoted.insert(*key, *triple);
    }
}
#[tokio::test]
async fn abstraction_uses_the_existing_triple_term_without_a_fresh_source_blank() {
    let (mut store, mut terms, prop) = fixture(false);
    let target = [[10, 20, 42], [42, TYPE, prop]];
    assert!(!match_target(&mut store, &terms, &target).await);
    type_propositions(&mut store, &mut terms).await;
    assert!(match_target(&mut store, &terms, &target).await);
    assert!(store.witnesses.is_empty());
    assert!(!store.asserted.contains(&[31, 32, 33]));
    assert!(!store.inferred.contains(&[31, 32, 33]));
    assert!(store.inferred.contains(&[90, TYPE, prop]));
}
#[tokio::test]
async fn nested_proposition_membership_preserves_mixed_outer_and_inner_blank_mapping() {
    let (mut store, mut terms, prop) = fixture(true);
    let target = [[10, 20, 93], [93, TYPE, prop], [41, 21, 34]];
    assert!(!match_target(&mut store, &terms, &target).await);
    type_propositions(&mut store, &mut terms).await;
    assert!(match_target(&mut store, &terms, &target).await);
    assert!(!match_target(&mut store, &terms, &[[10, 20, 93], [41, 21, 99]]).await);
    assert!(!match_target(&mut store, &terms, &[[10, 20, 94]]).await);
    for triple in [[31, 32, 33], [90, 22, 31]] {
        assert!(!store.asserted.contains(&triple));
        assert!(!store.inferred.contains(&triple));
    }
    assert!(store.inferred.contains(&[90, TYPE, prop]));
    assert!(store.inferred.contains(&[92, TYPE, prop]));
}
#[tokio::test]
async fn repeated_triple_terms_share_one_consistent_target_binding() {
    let (mut store, mut terms, prop) = fixture(false);
    store.asserted.insert([11, 20, 90]);
    type_propositions(&mut store, &mut terms).await;
    assert!(
        match_target(
            &mut store,
            &terms,
            &[[10, 20, 42], [11, 20, 42], [42, TYPE, prop]]
        )
        .await
    );
    assert!(
        match_target(
            &mut store,
            &terms,
            &[[10, 20, 91], [11, 20, 91], [91, TYPE, prop], [41, 21, 34]]
        )
        .await
    );
    assert!(
        !match_target(
            &mut store,
            &terms,
            &[[10, 20, 91], [11, 20, 91], [41, 21, 99]]
        )
        .await
    );
}
