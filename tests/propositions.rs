//! Generalized proposition construction is one bounded phase, never recursive closure.
#[allow(
    dead_code,
    reason = "shared capped adapter also serves other reasoning phases"
)]
mod support;
use inference_engine::reasoning::{
    Evidence, Inference, Premises, PropositionLimits, Rdf12PropositionConstructor,
    Rdf12TripleTermStore, ReasoningStore,
};
use rdf_reasoning::{Rdf12PropositionRuleId, RuleId};
use std::num::NonZeroU64;
use support::{Store, TYPE};

fn constructor(inferred: u64, matches: u64) -> Rdf12PropositionConstructor {
    Rdf12PropositionConstructor::new(PropositionLimits::new(
        NonZeroU64::new(inferred).unwrap(),
        NonZeroU64::new(matches).unwrap(),
    ))
}
fn fixture() -> (Store, u64, u64, u64) {
    let mut store = Store::new([]);
    let property = store.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#Property");
    let resource = store.iri("http://www.w3.org/2000/01/rdf-schema#Resource");
    let proposition = store.iri("http://www.w3.org/2000/01/rdf-schema#Proposition");
    store.asserted.extend([
        [20, TYPE, property],
        [10, TYPE, resource],
        [11, TYPE, resource],
    ]);
    (store, property, resource, proposition)
}

#[tokio::test]
async fn constructs_from_three_shared_premises_with_one_immutable_snapshot() {
    let (mut store, property, resource, proposition) = fixture();
    let report = constructor(4, 7).run(&mut store).await.unwrap();
    assert_eq!(
        (report.inferred, report.matches, store.snapshot_calls),
        (4, 7, 1)
    );
    let term = store.constructed[&[10, 20, 11]];
    let inference = store
        .evidence
        .iter()
        .find(|e| e.triple == [term, TYPE, proposition])
        .unwrap();
    assert!(
        matches!(&inference.evidence, Evidence::Proposition { rule: Rdf12PropositionRuleId::Grdfs14, premises }
        if *premises == [[20, TYPE, property], [10, TYPE, resource], [11, TYPE, resource]])
    );
    assert!(
        !store.inferred.contains(&[10, 20, 11]),
        "quoted contents were asserted"
    );
    assert_eq!(
        serde_json::from_slice::<Inference<u64>>(&serde_json::to_vec(inference).unwrap()).unwrap(),
        *inference
    );
    assert_eq!(
        postcard::from_bytes::<Inference<u64>>(&postcard::to_allocvec(inference).unwrap()).unwrap(),
        *inference
    );
}

#[tokio::test]
async fn range_derived_literal_property_can_bind_the_generalized_predicate_slot() {
    let (mut store, property, resource, proposition) = fixture();
    store.asserted.remove(&[20, TYPE, property]);
    let range = store.iri("http://www.w3.org/2000/01/rdf-schema#range");
    store.asserted.extend([[21, range, property], [10, 21, 50]]);
    // The preceding core phase can derive literal 50's property membership through rdfs3.
    store
        .insert_inferred(Inference {
            triple: [50, TYPE, property],
            evidence: Evidence::Rule {
                rule: RuleId::Rdfs3,
                premises: Premises::Two([[21, range, property], [10, 21, 50]]),
            },
        })
        .await
        .unwrap();
    constructor(4, 7).run(&mut store).await.unwrap();
    let term = store.constructed[&[10, 50, 11]];
    assert!(store.inferred.contains(&[term, TYPE, proposition]));
    assert!(!store.inferred.contains(&[10, 50, 11]));
    assert!(store.asserted.contains(&[11, TYPE, resource]));
}

#[tokio::test]
async fn preserves_asserted_duplicates_and_exact_budget_can_complete() {
    let (mut store, _, _, proposition) = fixture();
    let term = store.resolve_triple_term([10, 20, 11]).await.unwrap();
    store.asserted.insert([term, TYPE, proposition]);
    let report = constructor(3, 7).run(&mut store).await.unwrap();
    assert_eq!(report.inferred, 3);
    assert!(!store.inferred.contains(&[term, TYPE, proposition]));
}

#[tokio::test]
async fn absent_premises_are_not_invented_and_new_heads_do_not_feed_back() {
    let (mut store, property, _, _) = fixture();
    store.asserted.remove(&[20, TYPE, property]);
    assert_eq!(
        constructor(4, 100).run(&mut store).await.unwrap().inferred,
        0
    );
    assert!(store.constructed.is_empty());
    let (mut store, _, _, _) = fixture();
    constructor(4, 7).run(&mut store).await.unwrap();
    assert_eq!(store.snapshot_calls, 1);
    assert_eq!(store.constructed.len(), 4);
    assert!(
        store
            .constructed
            .keys()
            .all(|terms| terms.iter().all(|t| *t < 500))
    );
}

#[tokio::test]
async fn output_work_constructor_depth_and_cancellation_failures_never_report_success() {
    for (inferred, matches) in [(1, 100), (4, 1)] {
        let (mut store, _, _, _) = fixture();
        assert!(
            constructor(inferred, matches)
                .run(&mut store)
                .await
                .is_err()
        );
        assert!(store.evidence.len() <= inferred as usize);
    }
    let (mut store, _, resource, _) = fixture();
    store.asserted.remove(&[10, TYPE, resource]);
    store.asserted.remove(&[11, TYPE, resource]);
    store.asserted.insert([299, TYPE, resource]);
    store.term_depths.insert(299, 4);
    let error = constructor(4, 100).run(&mut store).await.unwrap_err();
    assert!(error.to_string().contains("nesting limit"));
    assert!(store.inferred.is_empty());
    let (mut store, _, _, _) = fixture();
    store.cancelled = true;
    assert!(constructor(4, 100).run(&mut store).await.is_err());
    assert!(store.constructed.is_empty());
    let (mut store, _, _, _) = fixture();
    store.fail_cursor = true;
    assert!(constructor(4, 100).run(&mut store).await.is_err());
}
