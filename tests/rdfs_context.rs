//! Target-relative context follows complete source preparation and diagnostics.
#[allow(dead_code)]
mod source_support;
use inference_engine::entailment::{EntailmentLimits, SimpleEntailment};
use inference_engine::reasoning::{
    Evidence, LiteralError, LiteralRole, LiteralTypingError, Rdfs12Context, Rdfs12ContextConfig,
    Rdfs12ContextError, Rdfs12ContextOutcome, Rdfs12SourceError, ReasoningStore, TripleSource,
};
use iri_rfc3987::Iri;
use rdf_reasoning::Rdf12Datatype;
use rdf_structural_model::Literal;
use source_support::{Diagnostics, Fixture, MemoryStore, context_config};
use std::{
    io,
    num::{NonZeroU32, NonZeroU64},
};

type Target = <MemoryStore as ReasoningStore>::Snapshot;
async fn target(rows: impl IntoIterator<Item = [u64; 3]>) -> Target {
    MemoryStore::new(rows).snapshot().await.unwrap()
}
struct UnreadableTarget;
impl TripleSource for UnreadableTarget {
    type Term = u64;
    type Error = io::Error;
    type Cursor = <Target as TripleSource>::Cursor;
    async fn scan(&self, _: [Option<u64>; 3]) -> io::Result<Self::Cursor> {
        Err(io::Error::other("target must not be read"))
    }
}

#[tokio::test]
async fn nested_target_vocabulary_seeds_containers_without_asserting_quotes_or_target_predicates() {
    let mut f = Fixture::new();
    let s = f.iri("urn:example:s");
    let claims = f.iri("urn:example:claims");
    let a = f.iri("urn:example:a");
    let b = f.iri("urn:example:b");
    let numbered = f.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#_937");
    let quote = f.quoted([a, numbered, b]).await;
    let root = [s, claims, quote];
    let target = target([root]).await;
    let mut diagnostics = Diagnostics::default();
    let result = Rdfs12Context::new(context_config())
        .run(&mut f.store, &f.access, &target, &mut diagnostics)
        .await
        .unwrap();
    let Rdfs12ContextOutcome::Prepared { source, target } = result else {
        panic!("unexpected source conflict")
    };
    assert_eq!(source.diagnostics, 0);
    assert_eq!(target.seeded, 4);
    assert_eq!(target.deferred_triple_terms, 1);
    let cmp = f.iri("http://www.w3.org/2000/01/rdf-schema#ContainerMembershipProperty");
    let property = f.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#Property");
    let resource = f.iri("http://www.w3.org/2000/01/rdf-schema#Resource");
    let member = f.iri("http://www.w3.org/2000/01/rdf-schema#member");
    for triple in [[numbered, 1, cmp], [claims, 1, resource], [a, 1, resource]] {
        assert!(f.store.inner.inferred.contains(&triple));
    }
    for triple in [
        root,
        [a, numbered, b],
        [a, member, b],
        [claims, 1, property],
    ] {
        assert!(!f.store.inner.inferred.contains(&triple));
    }
    assert!(f.store.inner.evidence.iter().any(|proof| matches!(&proof.evidence,
        Evidence::Occurrence { occurrence, .. } if occurrence.root == root && occurrence.role == LiteralRole::TargetContext)));
    assert!(diagnostics.0.is_empty());
}

#[tokio::test]
async fn supported_source_conflicts_finish_before_any_target_read() {
    let mut f = Fixture::new();
    let string = f.iri(Rdf12Datatype::String.iri());
    let lang = f.iri(Rdf12Datatype::LangString.iri());
    f.store.inner.asserted.insert([string, 2, lang]);
    let mut diagnostics = Diagnostics::default();
    let result = Rdfs12Context::new(context_config())
        .run(&mut f.store, &f.access, &UnreadableTarget, &mut diagnostics)
        .await
        .unwrap();
    assert!(
        matches!(result, Rdfs12ContextOutcome::SourceConflicting(report) if report.diagnostics > 0)
    );
    assert!(!diagnostics.0.is_empty());
}

#[tokio::test]
async fn target_literal_errors_retain_the_target_role_and_never_become_source_conflicts() {
    let mut f = Fixture::new();
    let s = f.iri("urn:example:s");
    let p = f.iri("urn:example:p");
    let bad = f.literal("\0");
    let root = [s, p, bad];
    let target = target([root]).await;
    let mut diagnostics = Diagnostics::default();
    let error = Rdfs12Context::new(context_config())
        .run(&mut f.store, &f.access, &target, &mut diagnostics)
        .await
        .unwrap_err();
    assert!(
        matches!(error, Rdfs12ContextError::Literals(LiteralTypingError::Literal { occurrence, error: LiteralError::Lexical { .. } })
        if occurrence.root == root && occurrence.role == LiteralRole::TargetContext)
    );
    assert!(diagnostics.0.is_empty());
}

#[tokio::test]
async fn late_target_cursor_failure_cannot_prepare_partial_context() {
    let mut f = Fixture::new();
    let numbered = f.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#_937");
    let s = f.iri("urn:example:s");
    let p = f.iri("urn:example:p");
    let mut broken = MemoryStore::new([[s, p, numbered]]);
    broken.fail_cursor = true;
    let target = broken.snapshot().await.unwrap();
    let result = Rdfs12Context::new(context_config())
        .run(
            &mut f.store,
            &f.access,
            &target,
            &mut Diagnostics::default(),
        )
        .await;
    assert!(matches!(result, Err(Rdfs12ContextError::Traversal(_))));
    let cmp = f.iri("http://www.w3.org/2000/01/rdf-schema#ContainerMembershipProperty");
    assert!(
        f.store.inner.inferred.contains(&[numbered, 1, cmp]),
        "failure followed target writes"
    );
}

#[tokio::test]
async fn target_literals_close_core_with_target_proofs_and_separate_source_counters() {
    let mut f = Fixture::new();
    let s = f.iri("urn:example:s");
    let p = f.iri("urn:example:p");
    let string = f.iri(Rdf12Datatype::String.iri());
    let class = f.iri("urn:example:TargetClass");
    let literal = f.literal("hello");
    f.store.inner.asserted.insert([string, 2, class]);
    let root = [s, p, literal];
    let result = Rdfs12Context::new(context_config())
        .run(
            &mut f.store,
            &f.access,
            &target([root]).await,
            &mut Diagnostics::default(),
        )
        .await
        .unwrap();
    let Rdfs12ContextOutcome::Prepared { source, target } = result else {
        panic!("unexpected conflict")
    };
    assert_eq!(source.lexical_bytes, 0);
    assert_eq!(source.literals, 0);
    assert_eq!(target.lexical_bytes, 5);
    assert_eq!(target.literals, 1);
    assert!(f.store.inner.inferred.contains(&[literal, 1, class]));
    assert!(!f.store.inner.inferred.contains(&root));
    assert!(f.store.inner.evidence.iter().any(|proof| matches!(&proof.evidence,
        Evidence::Literal { occurrence, .. } if occurrence.root == root && occurrence.role == LiteralRole::TargetContext)));
    let encoded = serde_json::to_string(&result).unwrap();
    assert_eq!(
        serde_json::from_str::<Rdfs12ContextOutcome>(&encoded).unwrap(),
        result
    );
    assert_eq!(
        postcard::from_bytes::<Rdfs12ContextOutcome>(&postcard::to_allocvec(&result).unwrap())
            .unwrap(),
        result
    );
    let config = serde_json::to_string(&context_config()).unwrap();
    assert!(
        serde_json::from_str::<Rdfs12ContextConfig>(&config.replacen("{", "{\"unknown\":1,", 1))
            .is_err()
    );
    let decoded: Rdfs12ContextConfig =
        postcard::from_bytes(&postcard::to_allocvec(&context_config()).unwrap()).unwrap();
    assert_eq!(serde_json::to_string(&decoded).unwrap(), config);
}

#[tokio::test]
async fn target_literal_datatype_metadata_is_not_appearing_container_vocabulary() {
    let mut f = Fixture::new();
    let numbered = "http://www.w3.org/1999/02/22-rdf-syntax-ns#_937";
    let metadata = Literal::typed("", Iri::parse(numbered).unwrap().into())
        .unwrap()
        .metadata()
        .clone();
    let literal = f.literal_metadata("opaque", metadata);
    let s = f.iri("urn:example:s");
    let p = f.iri("urn:example:p");
    let result = Rdfs12Context::new(context_config())
        .run(
            &mut f.store,
            &f.access,
            &target([[s, p, literal]]).await,
            &mut Diagnostics::default(),
        )
        .await
        .unwrap();
    let Rdfs12ContextOutcome::Prepared { target, .. } = result else {
        panic!("unexpected conflict")
    };
    assert_eq!(target.seeded, 0);
    let key = f.iri(numbered);
    let cmp = f.iri("http://www.w3.org/2000/01/rdf-schema#ContainerMembershipProperty");
    assert!(!f.store.inner.inferred.contains(&[key, 1, cmp]));
    assert!(!f.store.inner.inferred.contains(&[literal, 1, key]));
}

#[tokio::test]
async fn source_errors_and_target_metadata_or_budget_failures_do_not_prepare_context() {
    let mut f = Fixture::new();
    let s = f.iri("urn:example:s");
    let p = f.iri("urn:example:p");
    let bad = f.literal("\0");
    f.store.inner.asserted.insert([s, p, bad]);
    assert!(matches!(
        Rdfs12Context::new(context_config())
            .run(
                &mut f.store,
                &f.access,
                &UnreadableTarget,
                &mut Diagnostics::default()
            )
            .await,
        Err(Rdfs12ContextError::Source(Rdfs12SourceError::Literals(_)))
    ));
    for corrupt_identity in [false, true] {
        let mut f = Fixture::new();
        let s = f.iri("urn:example:s");
        let p = f.iri("urn:example:p");
        let numbered = f.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#_937");
        let mut config = context_config();
        if corrupt_identity {
            f.corrupt_container_classification();
        } else {
            config.target.axioms = NonZeroU64::new(1).unwrap();
        }
        let result = Rdfs12Context::new(config)
            .run(
                &mut f.store,
                &f.access,
                &target([[s, p, numbered]]).await,
                &mut Diagnostics::default(),
            )
            .await;
        if corrupt_identity {
            assert!(matches!(
                result,
                Err(Rdfs12ContextError::VocabularyIdentity)
            ));
        } else {
            assert!(matches!(result, Err(Rdfs12ContextError::Reasoning(_))));
        }
    }
}

#[tokio::test]
async fn an_unconstructed_target_triple_term_cannot_create_a_self_supporting_proof() {
    let mut f = Fixture::new();
    let resource = f.iri("http://www.w3.org/2000/01/rdf-schema#Resource");
    let class = f.iri("http://www.w3.org/2000/01/rdf-schema#Class");
    let property = f.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#Property");
    let a = f.iri("urn:example:a");
    let q = f.iri("urn:example:q");
    let b = f.iri("urn:example:b");
    let quote = f.quoted([a, q, b]).await;
    let blank = f.blank(1, "target").unwrap();
    f.store.inner.asserted.insert([resource, 2, class]);
    let target = target([[blank, 2, quote]]).await;
    Rdfs12Context::new(context_config())
        .run(
            &mut f.store,
            &f.access,
            &target,
            &mut Diagnostics::default(),
        )
        .await
        .unwrap();
    assert!(!f.store.inner.inferred.contains(&[q, 1, property]));
    assert!(!f.store.inner.inferred.contains(&[quote, 1, resource]));
    let source = f.store.snapshot().await.unwrap();
    let result = SimpleEntailment::new(EntailmentLimits::new(
        NonZeroU32::new(8).unwrap(),
        NonZeroU32::new(16).unwrap(),
        NonZeroU32::new(256).unwrap(),
        NonZeroU64::new(100_000).unwrap(),
    ))
    .check(&source, &target, &f.access)
    .await
    .unwrap();
    // A model may make every resource a class while keeping q outside IP.
    // IT's domain is IR x IP x IR, so this target triple term need not denote.
    assert!(
        !result.entailed,
        "target resource typing must not supply its own missing denotation premise"
    );
}
