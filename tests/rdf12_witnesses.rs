//! Nonempty-class witnesses are fresh scoped blanks, not fixed ground IRIs.
#[allow(
    dead_code,
    reason = "shared capped graph adapter also supplies core-rule tests"
)]
mod support;
use inference_engine::{
    entailment::{EntailmentLimits, EntailmentTerms, SimpleEntailment},
    reasoning::{AxiomFamily, Evidence, Rdf12AxiomSeeder, ReasoningStore},
};
use rdf_reasoning::Rdf12ExistentialAxiomId;
use std::{
    io,
    num::{NonZeroU32, NonZeroU64},
};
use support::{Store, TYPE};

struct Terms;
impl EntailmentTerms for Terms {
    type Term = u64;
    type Error = io::Error;
    async fn is_blank(&self, term: &u64) -> io::Result<bool> {
        Ok((100..400).contains(term))
    }
    async fn triple_term(&self, _: &u64) -> io::Result<Option<[u64; 3]>> {
        Ok(None)
    }
    fn is_cancelled(&self) -> bool {
        false
    }
}
fn seeder(limit: u64) -> Rdf12AxiomSeeder {
    Rdf12AxiomSeeder::new(NonZeroU64::new(limit).unwrap())
}
fn checker() -> SimpleEntailment {
    SimpleEntailment::new(EntailmentLimits::new(
        NonZeroU32::new(16).unwrap(),
        NonZeroU32::new(16).unwrap(),
        NonZeroU32::new(16).unwrap(),
        NonZeroU64::new(10_000).unwrap(),
    ))
}

#[tokio::test]
async fn empty_source_entails_existential_membership_but_never_fixed_user_iri_membership() {
    let mut store = Store::new([]);
    let mut seeds = seeder(4);
    for &id in Rdf12ExistentialAxiomId::ALL {
        seeds.seed_existential(&mut store, id).await.unwrap();
    }
    assert_eq!(seeds.inferred(), 4);
    assert!(store.asserted.is_empty());
    assert_eq!(store.witnesses.len(), 4);
    for &id in Rdf12ExistentialAxiomId::ALL {
        let class = store.iri(id.class_iri());
        let source = store.snapshot().await.unwrap();
        let existential = Store::new([[300, TYPE, class]]).snapshot().await.unwrap();
        assert!(
            checker()
                .check(&source, &existential, &Terms)
                .await
                .unwrap()
                .entailed
        );
        let fixed_iri = Store::new([[20, TYPE, class]]).snapshot().await.unwrap();
        assert!(
            !checker()
                .check(&source, &fixed_iri, &Terms)
                .await
                .unwrap()
                .entailed
        );
        let witness = store.witnesses[&id];
        assert!(
            store
                .evidence
                .iter()
                .any(|e| e.triple == [witness, TYPE, class]
                    && e.evidence == Evidence::Axiom(AxiomFamily::Rdf12Existential(id)))
        );
    }
}

#[tokio::test]
async fn repeated_seeding_reuses_persisted_scoped_binding_without_more_output_budget() {
    let mut store = Store::new([]);
    let mut seeds = seeder(4);
    for _ in 0..3 {
        for &id in Rdf12ExistentialAxiomId::ALL {
            seeds.seed_existential(&mut store, id).await.unwrap();
        }
    }
    assert_eq!(
        (seeds.inferred(), store.inferred.len(), store.evidence.len()),
        (4, 4, 4)
    );
    for &id in Rdf12ExistentialAxiomId::ALL {
        let mut distinct = Store::new([]);
        distinct.witness_scope = 1;
        seeder(1).seed_existential(&mut distinct, id).await.unwrap();
        assert_ne!(store.witnesses[&id], distinct.witnesses[&id]);
    }
    for evidence in &store.evidence {
        assert_eq!(
            postcard::from_bytes::<inference_engine::reasoning::Inference<u64>>(
                &postcard::to_allocvec(evidence).unwrap()
            )
            .unwrap(),
            *evidence
        );
        assert_eq!(
            serde_json::from_slice::<inference_engine::reasoning::Inference<u64>>(
                &serde_json::to_vec(evidence).unwrap()
            )
            .unwrap(),
            *evidence
        );
    }
}

#[tokio::test]
async fn witness_limit_cancellation_and_storage_failures_leave_only_unpublished_staging() {
    let mut store = Store::new([]);
    let mut seeds = seeder(3);
    for &id in &Rdf12ExistentialAxiomId::ALL[..3] {
        seeds.seed_existential(&mut store, id).await.unwrap();
    }
    assert!(
        seeds
            .seed_existential(&mut store, Rdf12ExistentialAxiomId::Rdfs14a)
            .await
            .is_err()
    );
    assert_eq!(store.inferred.len(), 3);
    for (cancelled, fail_write) in [(true, false), (false, true)] {
        let mut store = Store::new([]);
        store.cancelled = cancelled;
        store.fail_write = fail_write;
        assert!(
            seeder(4)
                .seed_existential(&mut store, Rdf12ExistentialAxiomId::Rdfs14a)
                .await
                .is_err()
        );
        assert!(store.inferred.is_empty());
    }
}
