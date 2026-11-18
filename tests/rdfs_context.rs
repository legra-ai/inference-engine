//! Target-relative context follows complete source preparation and diagnostics.
#[allow(dead_code)]
mod source_support;
use inference_engine::reasoning::{
    Evidence, LiteralRole, LiteralTypingError, Rdfs12Context, Rdfs12ContextError,
    Rdfs12ContextOutcome, ReasoningStore, TripleSource,
};
use rdf_reasoning::Rdf12Datatype;
use source_support::{Diagnostics, Fixture, MemoryStore, context_config};
use std::io;

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
        matches!(error, Rdfs12ContextError::Literals(LiteralTypingError::Literal { occurrence, .. })
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
