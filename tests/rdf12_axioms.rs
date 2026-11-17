//! Explicit shared axiom seeding is distinct from rule execution and full entailment.

mod support;
use inference_engine::reasoning::{
    AxiomFamily, Evidence, ExecutionProfile, Inference, Limit, Rdf12AxiomSeeder, Reasoner,
    ReasoningError,
};
use rdf_reasoning::{Rdf12AxiomId, Rdf12ContainerProperty};
use std::num::NonZeroU64;
use support::{Store, TYPE, limits};

fn seeder(limit: u64) -> Rdf12AxiomSeeder {
    Rdf12AxiomSeeder::new(NonZeroU64::new(limit).unwrap())
}

#[tokio::test]
async fn empty_source_receives_exact_shared_families_and_axiom_provenance() {
    let mut store = Store::new([]);
    let mut seeds = seeder(53);
    for (family, count) in [
        (Rdf12AxiomId::Rdf, 9),
        (Rdf12AxiomId::Rdfs, 41),
        (Rdf12AxiomId::DefaultDatatypes, 3),
    ] {
        let before = seeds.inferred();
        seeds.seed_family(&mut store, family).await.unwrap();
        assert_eq!(seeds.inferred() - before, count);
        assert!(
            store
                .evidence
                .iter()
                .skip(before as usize)
                .all(|i| { i.evidence == Evidence::Axiom(AxiomFamily::Rdf12(family)) })
        );
    }
    assert_eq!(store.asserted.len(), 0);
    assert_eq!(store.inferred.len(), 53);
    let reifies = store.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies");
    let range = store.iri("http://www.w3.org/2000/01/rdf-schema#range");
    let proposition = store.iri("http://www.w3.org/2000/01/rdf-schema#Proposition");
    assert!(store.inferred.contains(&[reifies, range, proposition]));
    let datatype = store.iri("http://www.w3.org/2000/01/rdf-schema#Datatype");
    let directional = store.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString");
    assert!(store.inferred.contains(&[directional, TYPE, datatype]));
    for &family in Rdf12AxiomId::ALL {
        seeds.seed_family(&mut store, family).await.unwrap();
    }
    assert_eq!(
        seeds.inferred(),
        53,
        "duplicate seeds consume no output budget"
    );
    assert_eq!(store.evidence.len(), 53);
}

#[tokio::test]
async fn container_seed_uses_unbounded_checked_index_and_drives_core_consequences() {
    let mut store = Store::new([]);
    let iri = "http://www.w3.org/1999/02/22-rdf-syntax-ns#_123456789012345678901234567890";
    let container = Rdf12ContainerProperty::try_new(iri).unwrap();
    let property = store.iri(iri);
    store.asserted.insert([10, property, 11]);
    let mut seeds = seeder(4);
    seeds.seed_container(&mut store, &container).await.unwrap();
    assert_eq!(seeds.inferred(), 4);
    assert!(
        store
            .evidence
            .iter()
            .all(|i| { i.evidence == Evidence::Axiom(AxiomFamily::Rdf12Container) })
    );
    Reasoner::new(ExecutionProfile::RdfsCore, limits(4096, 64, 1_000_000))
        .run(&mut store)
        .await
        .unwrap();
    let member = store.iri("http://www.w3.org/2000/01/rdf-schema#member");
    assert!(store.inferred.contains(&[10, member, 11]));
    assert!(
        store.evidence.iter().any(|i| {
            i.triple == [10, member, 11] && matches!(i.evidence, Evidence::Rule { .. })
        })
    );
}

#[tokio::test]
async fn assertions_win_and_exact_budget_permits_duplicate_seeding() {
    let mut store = Store::new([]);
    let container =
        Rdf12ContainerProperty::try_new("http://www.w3.org/1999/02/22-rdf-syntax-ns#_1").unwrap();
    let property = store.iri(container.iri());
    let class = store.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#Property");
    store.asserted.insert([property, TYPE, class]);
    let mut seeds = seeder(3);
    seeds.seed_container(&mut store, &container).await.unwrap();
    seeds.seed_container(&mut store, &container).await.unwrap();
    assert_eq!(seeds.inferred(), 3);
    assert!(!store.inferred.contains(&[property, TYPE, class]));
    assert_eq!(store.evidence.len(), 3);
}

#[tokio::test]
async fn shared_budget_cancellation_and_backend_errors_abort_seeding() {
    let mut store = Store::new([]);
    let mut seeds = seeder(10);
    seeds
        .seed_family(&mut store, Rdf12AxiomId::Rdf)
        .await
        .unwrap();
    assert!(matches!(
        seeds
            .seed_family(&mut store, Rdf12AxiomId::DefaultDatatypes)
            .await,
        Err(ReasoningError::LimitExceeded(Limit::Inferred))
    ));
    assert_eq!(store.inferred.len(), 10);
    assert_eq!(seeds.inferred(), 10);
    for (cancelled, fail_write) in [(true, false), (false, true)] {
        let mut store = Store::new([]);
        store.cancelled = cancelled;
        store.fail_write = fail_write;
        assert!(
            seeder(53)
                .seed_family(&mut store, Rdf12AxiomId::Rdf)
                .await
                .is_err()
        );
        assert!(store.inferred.is_empty());
    }
}

#[test]
fn axiom_evidence_round_trips_without_a_fictitious_rule_or_premise() {
    for family in [
        AxiomFamily::Rdf12(Rdf12AxiomId::Rdf),
        AxiomFamily::Rdf12(Rdf12AxiomId::Rdfs),
        AxiomFamily::Rdf12(Rdf12AxiomId::DefaultDatatypes),
        AxiomFamily::Rdf12Container,
    ] {
        let inference = Inference {
            triple: [1_u64, 2, 3],
            evidence: Evidence::Axiom(family),
        };
        assert_eq!(
            serde_json::from_slice::<Inference<u64>>(&serde_json::to_vec(&inference).unwrap())
                .unwrap(),
            inference
        );
        assert_eq!(
            postcard::from_bytes::<Inference<u64>>(&postcard::to_allocvec(&inference).unwrap())
                .unwrap(),
            inference
        );
    }
}
