//! Automatic target substitutions must drive finite, snapshot-isolated construction.
#[allow(dead_code)]
mod source_support;
use inference_engine::{
    entailment::{EntailmentLimits, SimpleEntailment},
    reasoning::{
        ExecutionProfile, PropositionLimits, Rdf12TargetConstructor, Rdfs12Context,
        Rdfs12ContextOutcome, Reasoner, ReasoningError, ReasoningStore, TargetConstructionError,
        TargetConstructionLimits,
    },
};
use source_support::{Diagnostics, Fixture, MemoryStore, context_config};
use std::num::{NonZeroU32, NonZeroU64};

type Target = <MemoryStore as ReasoningStore>::Snapshot;
fn n(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap()
}
fn matching() -> EntailmentLimits {
    EntailmentLimits::new(
        NonZeroU32::new(16).unwrap(),
        NonZeroU32::new(32).unwrap(),
        NonZeroU32::new(128).unwrap(),
        n(2_000_000),
    )
}
fn constructor() -> Rdf12TargetConstructor {
    Rdf12TargetConstructor::new(TargetConstructionLimits::new(
        matching(),
        PropositionLimits::new(n(1000), n(2_000_000)),
        NonZeroU32::new(128).unwrap(),
        n(2_000_000),
    ))
}
async fn matches(f: &mut Fixture, target: &Target) -> bool {
    SimpleEntailment::new(matching())
        .check(&f.store.snapshot().await.unwrap(), target, &f.access)
        .await
        .unwrap()
        .entailed
}
async fn prepare(f: &mut Fixture, target: &Target) {
    assert!(matches!(
        Rdfs12Context::new(context_config())
            .run(&mut f.store, &f.access, target, &mut Diagnostics::default())
            .await
            .unwrap(),
        Rdfs12ContextOutcome::Prepared { .. }
    ));
}
async fn close(f: &mut Fixture) {
    Reasoner::new(
        ExecutionProfile::RdfsCore,
        context_config().target.structural,
    )
    .run(&mut f.store)
    .await
    .unwrap();
}
async fn fixture(chain: bool, typed: bool) -> (Fixture, Target, [u64; 3], [u64; 3]) {
    let mut f = Fixture::new();
    let root = f.iri("urn:example:root");
    let rel = f.iri("urn:example:rel");
    let d = f.iri("urn:example:d");
    let p = f.iri("urn:example:p");
    let b = f.iri("urn:example:b");
    let c = f.iri("urn:example:c");
    let rdf_type = f.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
    let property = f.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#Property");
    let proposition = f.iri("http://www.w3.org/2000/01/rdf-schema#Proposition");
    let subclass = f.iri("http://www.w3.org/2000/01/rdf-schema#subClassOf");
    let subproperty = f.iri("http://www.w3.org/2000/01/rdf-schema#subPropertyOf");
    f.store
        .inner
        .asserted
        .extend([[root, rel, d], [proposition, subclass, property]]);
    if typed {
        f.store.inner.asserted.insert([p, rdf_type, property]);
    }
    let z = f.blank(1, "z").unwrap();
    let x = f.blank(1, "x").unwrap();
    let y = f.blank(1, "y").unwrap();
    let first = f.quoted([z, p, b]).await;
    // bounded: at most five target patterns in this legal coupling/chain fixture.
    let mut rows = vec![
        [root, rel, z],
        [x, subproperty, first],
        [x, rdf_type, proposition],
    ];
    if chain {
        let second = f.quoted([x, p, c]).await;
        rows.extend([[y, subproperty, second], [y, rdf_type, proposition]]);
    }
    let target = MemoryStore::new(rows).snapshot().await.unwrap();
    (f, target, [d, p, b], [x, p, c])
}

#[tokio::test]
async fn coupled_goal_uses_the_binding_from_another_target_pattern() {
    let (mut f, target, candidate, _) = fixture(false, true).await;
    prepare(&mut f, &target).await;
    assert!(!matches(&mut f, &target).await);
    let report = constructor()
        .run(&mut f.store, &f.access, &target)
        .await
        .unwrap();
    assert!(report.propositions.inferred > 0);
    assert!(f.store.inner.constructed.contains_key(&candidate));
    assert!(!matches(&mut f, &target).await);
    close(&mut f).await;
    assert!(matches(&mut f, &target).await);
    assert!(!f.store.inner.inferred.contains(&candidate));
}

#[tokio::test]
async fn chained_goal_requires_a_second_snapshot_after_core_closure() {
    let (mut f, target, first, mut second) = fixture(true, true).await;
    prepare(&mut f, &target).await;
    let snapshots = f.store.inner.snapshot_calls;
    constructor()
        .run(&mut f.store, &f.access, &target)
        .await
        .unwrap();
    assert_eq!(f.store.inner.snapshot_calls, snapshots + 1);
    let key = f.store.inner.constructed[&first];
    second[0] = key;
    let property = f.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#Property");
    assert!(!f.store.inner.inferred.contains(&[key, 1, property]));
    assert!(!matches(&mut f, &target).await);
    close(&mut f).await;
    assert!(!matches(&mut f, &target).await);
    constructor()
        .run(&mut f.store, &f.access, &target)
        .await
        .unwrap();
    close(&mut f).await;
    assert!(matches(&mut f, &target).await);
    assert!(f.store.inner.constructed.contains_key(&second));
    assert!(!f.store.inner.inferred.contains(&first));
    assert!(!f.store.inner.inferred.contains(&second));
}

#[tokio::test]
async fn target_only_predicate_does_not_acquire_a_property_premise() {
    let (mut f, target, candidate, _) = fixture(false, false).await;
    prepare(&mut f, &target).await;
    let report = constructor()
        .run(&mut f.store, &f.access, &target)
        .await
        .unwrap();
    assert_eq!(report.propositions.inferred, 0);
    close(&mut f).await;
    assert!(!matches(&mut f, &target).await);
    let property = f.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#Property");
    assert!(
        !f.store
            .contains(&[candidate[1], 1, property])
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn unanchored_blanks_share_a_selected_noninjective_representative() {
    let (mut f, _, _, _) = fixture(false, true).await;
    let p = f.iri("urn:example:p");
    let rdf_type = f.iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
    let proposition = f.iri("http://www.w3.org/2000/01/rdf-schema#Proposition");
    let subproperty = f.iri("http://www.w3.org/2000/01/rdf-schema#subPropertyOf");
    let a = f.blank(1, "unbound-a").unwrap();
    let b = f.blank(1, "unbound-b").unwrap();
    let x = f.blank(1, "unbound-x").unwrap();
    let quoted = f.quoted([a, p, b]).await;
    let target = MemoryStore::new([[x, subproperty, quoted], [x, rdf_type, proposition]])
        .snapshot()
        .await
        .unwrap();
    prepare(&mut f, &target).await;
    constructor()
        .run(&mut f.store, &f.access, &target)
        .await
        .unwrap();
    close(&mut f).await;
    assert!(matches(&mut f, &target).await);
    assert!(
        f.store
            .inner
            .constructed
            .contains_key(&[rdf_type, p, rdf_type])
    );
    assert!(!f.store.inner.inferred.contains(&[a, p, b]));
}

#[tokio::test]
async fn construction_preserves_the_unestablished_target_denotation_guard() {
    let mut f = Fixture::new();
    let resource = f.iri("http://www.w3.org/2000/01/rdf-schema#Resource");
    let class = f.iri("http://www.w3.org/2000/01/rdf-schema#Class");
    let subclass = f.iri("http://www.w3.org/2000/01/rdf-schema#subClassOf");
    let a = f.iri("urn:example:a");
    let q = f.iri("urn:example:untyped-q");
    let b = f.iri("urn:example:b");
    let quoted = f.quoted([a, q, b]).await;
    let v = f.blank(1, "v").unwrap();
    f.store.inner.asserted.insert([resource, subclass, class]);
    let target = MemoryStore::new([[v, subclass, quoted]])
        .snapshot()
        .await
        .unwrap();
    prepare(&mut f, &target).await;
    let report = constructor()
        .run(&mut f.store, &f.access, &target)
        .await
        .unwrap();
    assert_eq!(report.propositions.inferred, 0);
    close(&mut f).await;
    assert!(!matches(&mut f, &target).await);
    assert!(!f.store.contains(&[quoted, 1, resource]).await.unwrap());
}

#[tokio::test]
async fn deeper_negative_search_keeps_storage_depth_failure_as_an_error() {
    let mut f = Fixture::new();
    let s = f.iri("urn:example:s");
    let claims = f.iri("urn:example:claims");
    let a = f.iri("urn:example:a");
    let p = f.iri("urn:example:p");
    let c = f.iri("urn:example:c");
    let q = f.iri("urn:example:q");
    let d = f.iri("urn:example:d");
    let inner = f.quoted([c, q, d]).await;
    let outer = f.quoted([a, p, inner]).await;
    f.store.inner.asserted.insert([s, claims, outer]);
    let v = f.blank(1, "v").unwrap();
    let pattern = f.quoted([a, p, v]).await;
    let proposition = f.iri("http://www.w3.org/2000/01/rdf-schema#Proposition");
    let target = MemoryStore::new([[s, claims, pattern], [s, claims, v], [v, 1, proposition]])
        .snapshot()
        .await
        .unwrap();
    prepare(&mut f, &target).await;
    for _ in 0..2 {
        constructor()
            .run(&mut f.store, &f.access, &target)
            .await
            .unwrap();
        close(&mut f).await;
        assert!(!matches(&mut f, &target).await);
    }
    let error = constructor()
        .run(&mut f.store, &f.access, &target)
        .await
        .unwrap_err();
    let TargetConstructionError::Reasoning(ReasoningError::Storage(error)) = error else {
        panic!("expected the backend's actual substituted-depth error")
    };
    assert_eq!(
        error.to_string(),
        "substituted triple-term nesting limit exceeded"
    );
}
