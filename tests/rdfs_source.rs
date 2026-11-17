//! Source preparation must complete before any target-context augmentation.
mod source_support;
use inference_engine::reasoning::{
    DatatypeConflictLimits, LiteralTypingLimits, OccurrenceLimits, PropositionLimits, Rdfs12Source,
    Rdfs12SourceConfig, Rdfs12SourceOutcome, RunLimits,
};
use rdf_reasoning::Rdf12Datatype;
use source_support::{Diagnostics, Fixture};
use std::num::{NonZeroU32, NonZeroU64};
fn n(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap()
}
fn config() -> Rdfs12SourceConfig {
    Rdfs12SourceConfig {
        axioms: n(128),
        rounds: NonZeroU32::new(16).unwrap(),
        structural: RunLimits::new(NonZeroU32::new(32).unwrap(), n(3000), n(1_000_000)),
        traversal: OccurrenceLimits::new(NonZeroU32::new(8).unwrap(), n(30_000), n(100_000)),
        literals: LiteralTypingLimits::new(n(10_000), n(3000), 1_000_000),
        propositions: PropositionLimits::new(n(3000), n(10_000)),
        conflicts: DatatypeConflictLimits::new(n(100), n(100_000)),
        iri_bytes: NonZeroU32::new(4096).unwrap(),
    }
}
#[tokio::test]
async fn prepares_inherited_membership_and_seeded_source_without_claiming_target_entailment() {
    let mut fixture = Fixture::new();
    let a = fixture.iri("urn:example:A");
    let b = fixture.iri("urn:example:B");
    let x = fixture.iri("urn:example:x");
    fixture.store.inner.asserted.extend([[a, 2, b], [x, 1, a]]);
    let mut diagnostics = Diagnostics::default();
    let result = Rdfs12Source::new(config())
        .run(&mut fixture.store, &fixture.access, &mut diagnostics)
        .await
        .unwrap();
    let Rdfs12SourceOutcome::Prepared(report) = result else {
        panic!("valid source reported a conflict")
    };
    assert!(fixture.store.inner.inferred.contains(&[x, 1, b]));
    assert!(report.rounds >= 2);
    assert!(report.seeded >= 57);
    assert!(diagnostics.0.is_empty());
}
#[tokio::test]
async fn closes_nested_literals_predicates_and_existing_propositions_together() {
    let mut fixture = Fixture::new();
    let subject = fixture.iri("urn:example:s");
    let claims = fixture.iri("urn:example:claims");
    let a = fixture.iri("urn:example:a");
    let p = fixture.iri("urn:example:p");
    let literal = fixture.literal("hello");
    let quoted = fixture.quoted([a, p, literal]).await;
    fixture
        .store
        .inner
        .asserted
        .insert([subject, claims, quoted]);
    let mut diagnostics = Diagnostics::default();
    assert!(matches!(
        Rdfs12Source::new(config())
            .run(&mut fixture.store, &fixture.access, &mut diagnostics)
            .await
            .unwrap(),
        Rdfs12SourceOutcome::Prepared(_)
    ));
    let property = fixture.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#Property");
    let proposition = fixture.iri("http://www.w3.org/2000/01/rdf-schema#Proposition");
    let string = fixture.iri(Rdf12Datatype::String.iri());
    for triple in [
        [p, 1, property],
        [quoted, 1, proposition],
        [literal, 1, string],
    ] {
        assert!(fixture.store.inner.inferred.contains(&triple));
    }
    assert!(!fixture.store.inner.inferred.contains(&[a, p, literal]));
}
#[tokio::test]
async fn discovers_source_only_container_vocabulary_and_derives_member() {
    let mut fixture = Fixture::new();
    let a = fixture.iri("urn:example:a");
    let b = fixture.iri("urn:example:b");
    let numbered = fixture.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#_937");
    fixture.store.inner.asserted.insert([a, numbered, b]);
    let mut diagnostics = Diagnostics::default();
    Rdfs12Source::new(config())
        .run(&mut fixture.store, &fixture.access, &mut diagnostics)
        .await
        .unwrap();
    let member = fixture.iri("http://www.w3.org/2000/01/rdf-schema#member");
    assert!(fixture.store.inner.inferred.contains(&[a, member, b]));
}
#[tokio::test]
async fn nonempty_datatype_witnesses_expose_source_schema_conflict_without_literal_instances() {
    let mut fixture = Fixture::new();
    let string = fixture.iri(Rdf12Datatype::String.iri());
    let language = fixture.iri(Rdf12Datatype::LangString.iri());
    fixture.store.inner.asserted.insert([string, 2, language]);
    let mut diagnostics = Diagnostics::default();
    let result = Rdfs12Source::new(config())
        .run(&mut fixture.store, &fixture.access, &mut diagnostics)
        .await
        .unwrap();
    let Rdfs12SourceOutcome::Conflicting(report) = result else {
        panic!("source conflict was missed")
    };
    assert!(report.diagnostics > 0);
    assert!(!diagnostics.0.is_empty());
}
#[tokio::test]
async fn lexical_budget_read_and_cancellation_failures_never_prepare_partial_source() {
    for failure in 0..4 {
        let mut fixture = Fixture::new();
        let mut limits = config();
        if failure == 0 {
            let literal = fixture.literal("bad\0string");
            let a = fixture.iri("urn:example:a");
            let p = fixture.iri("urn:example:p");
            fixture.store.inner.asserted.insert([a, p, literal]);
        }
        if failure == 1 {
            limits.axioms = n(1);
        }
        if failure == 2 {
            fixture.store.inner.fail_cursor = true;
        }
        if failure == 3 {
            fixture.store.inner.cancelled = true;
        }
        let mut diagnostics = Diagnostics::default();
        assert!(
            Rdfs12Source::new(limits)
                .run(&mut fixture.store, &fixture.access, &mut diagnostics)
                .await
                .is_err()
        );
    }
}
