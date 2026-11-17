//! Shared occurrence rules bind authoritative structure rather than RDF lookalikes.
#[allow(dead_code)]
mod support;
use inference_engine::{
    entailment::EntailmentTerms,
    reasoning::{
        Evidence, LiteralRole, OccurrenceLimits, OccurrenceTypingError, Rdf12OccurrenceCursor,
        Rdf12OccurrenceTyper, ReasoningStore, TripleSource,
    },
};
use rdf_reasoning::Rdf12OccurrenceRuleId as R;
use std::{
    io,
    num::{NonZeroU32, NonZeroU64},
};
use support::{Store, TYPE};
struct Terms;
impl EntailmentTerms for Terms {
    type Term = u64;
    type Error = io::Error;
    async fn is_blank(&self, _: &u64) -> io::Result<bool> {
        Ok(false)
    }
    async fn triple_term(&self, key: &u64) -> io::Result<Option<[u64; 3]>> {
        Ok(if *key == 90 { Some([10, 11, 12]) } else { None })
    }
    fn is_cancelled(&self) -> bool {
        false
    }
}
fn limits() -> OccurrenceLimits {
    OccurrenceLimits::new(
        NonZeroU32::new(2).unwrap(),
        NonZeroU64::new(30).unwrap(),
        NonZeroU64::new(100).unwrap(),
    )
}
#[tokio::test]
async fn source_types_nested_predicates_and_all_terms_with_occurrence_evidence() {
    let mut store = Store::new([[7, 8, 90]]);
    let roots = store
        .snapshot()
        .await
        .unwrap()
        .scan([None; 3])
        .await
        .unwrap();
    let mut cursor = Rdf12OccurrenceCursor::new(roots, &Terms, LiteralRole::Source, limits());
    let report = Rdf12OccurrenceTyper::new(NonZeroU64::new(8).unwrap(), LiteralRole::Source)
        .run(&mut store, &mut cursor)
        .await
        .unwrap();
    assert_eq!(report.occurrences, 6);
    assert_eq!(report.inferred, 8);
    let property = store.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#Property");
    let resource = store.iri("http://www.w3.org/2000/01/rdf-schema#Resource");
    for key in [7, 8, 90, 10, 11, 12] {
        assert!(store.inferred.contains(&[key, TYPE, resource]));
    }
    for key in [8, 11] {
        assert!(store.inferred.contains(&[key, TYPE, property]));
    }
    assert!(!store.inferred.contains(&[10, 11, 12]));
    let proof = store
        .evidence
        .iter()
        .find(|i| i.triple == [11, TYPE, property])
        .unwrap();
    let Evidence::Occurrence { rule, occurrence } = &proof.evidence else {
        panic!("fabricated RDF premise")
    };
    assert_eq!(*rule, R::RdfD2);
    assert_eq!(occurrence.root, [7, 8, 90]);
    assert_eq!(occurrence.parent, [10, 11, 12]);
    assert!(occurrence.is_predicate());
    assert_eq!(
        postcard::from_bytes::<inference_engine::reasoning::Inference<u64>>(
            &postcard::to_allocvec(proof).unwrap()
        )
        .unwrap(),
        *proof
    );
    assert_eq!(
        serde_json::from_str::<inference_engine::reasoning::Inference<u64>>(
            &serde_json::to_string(proof).unwrap()
        )
        .unwrap(),
        *proof
    );
}
#[tokio::test]
async fn target_context_cannot_type_its_predicates_as_source_properties() {
    let mut store = Store::new([[7, 8, 90]]);
    let roots = store
        .snapshot()
        .await
        .unwrap()
        .scan([None; 3])
        .await
        .unwrap();
    let mut cursor =
        Rdf12OccurrenceCursor::new(roots, &Terms, LiteralRole::TargetContext, limits());
    let report = Rdf12OccurrenceTyper::new(NonZeroU64::new(6).unwrap(), LiteralRole::TargetContext)
        .run(&mut store, &mut cursor)
        .await
        .unwrap();
    assert_eq!(report.inferred, 6);
    assert!(store.evidence.iter().all(|i|matches!(&i.evidence,Evidence::Occurrence{rule:R::Rdfs4,occurrence} if occurrence.role==LiteralRole::TargetContext)));
    let property = store.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#Property");
    assert!(!store.inferred.contains(&[8, TYPE, property]));
    assert!(!store.inferred.contains(&[11, TYPE, property]));
}
#[tokio::test]
async fn role_mismatch_is_rejected_before_roots_are_read() {
    let mut store = Store::new([]);
    store.fail_cursor = true;
    let roots = store
        .snapshot()
        .await
        .unwrap()
        .scan([None; 3])
        .await
        .unwrap();
    let mut cursor =
        Rdf12OccurrenceCursor::new(roots, &Terms, LiteralRole::TargetContext, limits());
    assert!(matches!(
        Rdf12OccurrenceTyper::new(NonZeroU64::new(1).unwrap(), LiteralRole::Source)
            .run(&mut store, &mut cursor)
            .await,
        Err(OccurrenceTypingError::RoleMismatch { .. })
    ));
    assert!(store.inferred.is_empty());
}
#[tokio::test]
async fn rdf_lookalikes_cannot_bind_virtual_occurrence_relations() {
    let mut store = Store::new([]);
    let fake = store.iri(rdf_reasoning::RDF12_APPEARING_TRIPLE_RELATION);
    let property = store.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#Property");
    store.asserted.insert([40, fake, 41]);
    let roots = store
        .snapshot()
        .await
        .unwrap()
        .scan([None; 3])
        .await
        .unwrap();
    let mut cursor = Rdf12OccurrenceCursor::new(roots, &Terms, LiteralRole::Source, limits());
    Rdf12OccurrenceTyper::new(NonZeroU64::new(10).unwrap(), LiteralRole::Source)
        .run(&mut store, &mut cursor)
        .await
        .unwrap();
    assert!(!store.inferred.contains(&[40, TYPE, property]));
    assert!(!store.inferred.contains(&[41, TYPE, property]));
    assert!(store.inferred.contains(&[fake, TYPE, property])); // Only its actual RDF predicate position qualifies.
}
#[tokio::test]
async fn duplicates_late_reads_writes_and_output_limits_preserve_attempt_failure() {
    for failure in 0..4 {
        let mut store = Store::new([[7, 8, 90]]);
        if failure == 0 {
            store.fail_cursor = true;
        }
        if failure == 1 {
            store.fail_write = true;
        }
        let roots = store
            .snapshot()
            .await
            .unwrap()
            .scan([None; 3])
            .await
            .unwrap();
        let mut cursor = Rdf12OccurrenceCursor::new(roots, &Terms, LiteralRole::Source, limits());
        let result = Rdf12OccurrenceTyper::new(
            NonZeroU64::new(if failure == 2 { 1 } else { 8 }).unwrap(),
            LiteralRole::Source,
        )
        .run(&mut store, &mut cursor)
        .await;
        if failure < 3 {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap().inferred, 8);
            let roots = store
                .snapshot()
                .await
                .unwrap()
                .scan([Some(7), Some(8), Some(90)])
                .await
                .unwrap();
            let mut cursor =
                Rdf12OccurrenceCursor::new(roots, &Terms, LiteralRole::Source, limits());
            assert_eq!(
                Rdf12OccurrenceTyper::new(NonZeroU64::new(1).unwrap(), LiteralRole::Source)
                    .run(&mut store, &mut cursor)
                    .await
                    .unwrap()
                    .inferred,
                0
            );
        }
    }
}
