//! Every selected RDFS core rule must produce its own semantic consequence.

mod support;
use inference_engine::reasoning::{Evidence, ExecutionProfile, Reasoner};
use rdf_reasoning::RuleId::*;
use rdf_reasoning::{Preset, RuleId};
use support::{SUBCLASS, Store, TYPE, limits};

fn execute() -> Reasoner {
    Reasoner::new(ExecutionProfile::RdfsCore, limits(4096, 64, 1_000_000))
}

fn rdf(store: &mut Store, local: &str) -> u64 {
    store.iri(&format!(
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#{local}"
    ))
}

fn rdfs(store: &mut Store, local: &str) -> u64 {
    store.iri(&format!("http://www.w3.org/2000/01/rdf-schema#{local}"))
}

fn fixture(store: &mut Store, rule: RuleId) -> [u64; 3] {
    let property = rdf(store, "Property");
    let class = rdfs(store, "Class");
    let resource = rdfs(store, "Resource");
    let subproperty = rdfs(store, "subPropertyOf");
    let domain = rdfs(store, "domain");
    let range = rdfs(store, "range");
    let member_property = rdfs(store, "ContainerMembershipProperty");
    let member = rdfs(store, "member");
    let datatype = rdfs(store, "Datatype");
    let literal = rdfs(store, "Literal");
    // bounded: each independently authored rule fixture has at most three facts.
    let (input, expected) = match rule {
        Rdf1 => (vec![[10, 11, 12]], [11, TYPE, property]),
        Rdfs2 => (vec![[11, domain, 20], [10, 11, 12]], [10, TYPE, 20]),
        Rdfs3 => (vec![[11, range, 20], [10, 11, 12]], [12, TYPE, 20]),
        Rdfs4a => (vec![[10, 11, 12]], [10, TYPE, resource]),
        Rdfs4b => (vec![[10, 11, 12]], [12, TYPE, resource]),
        Rdfs5 => (
            vec![[10, subproperty, 11], [11, subproperty, 12]],
            [10, subproperty, 12],
        ),
        Rdfs6 => (vec![[10, TYPE, property]], [10, subproperty, 10]),
        Rdfs7 => (vec![[11, subproperty, 20], [10, 11, 12]], [10, 20, 12]),
        Rdfs8 => (vec![[10, TYPE, class]], [10, SUBCLASS, resource]),
        Rdfs9 => (vec![[11, SUBCLASS, 20], [10, TYPE, 11]], [10, TYPE, 20]),
        Rdfs10 => (vec![[10, TYPE, class]], [10, SUBCLASS, 10]),
        Rdfs11 => (
            vec![[10, SUBCLASS, 11], [11, SUBCLASS, 12]],
            [10, SUBCLASS, 12],
        ),
        Rdfs12 => (vec![[10, TYPE, member_property]], [10, subproperty, member]),
        Rdfs13 => (vec![[10, TYPE, datatype]], [10, SUBCLASS, literal]),
        _ => panic!("non-RDFS core rule in fixture: {rule:?}"),
    };
    store.asserted.extend(input);
    expected
}

#[tokio::test]
async fn executes_each_of_the_fourteen_shared_rules_with_exact_witness_arity() {
    assert_eq!(ExecutionProfile::RdfsCore.rules(), Preset::RdfsCore.rules());
    assert_eq!(
        ExecutionProfile::RdfsCore.exclusions(),
        Preset::RdfsCore.exclusions()
    );
    assert_eq!(ExecutionProfile::RdfsCore.rules().len(), 14);
    for &rule in Preset::RdfsCore.rules() {
        let mut store = Store::new([]);
        let expected = fixture(&mut store, rule);
        execute().run(&mut store).await.unwrap();
        assert!(
            store.inferred.contains(&expected),
            "missing {rule:?}: {expected:?}"
        );
        assert!(
            store.evidence.iter().any(|e| e.triple == expected
                && matches!(e.evidence, Evidence::Rule { rule: id, .. } if id == rule)),
            "missing rule witness for {rule:?}"
        );
        for evidence in &store.evidence {
            let Evidence::Rule { rule, premises } = &evidence.evidence else {
                panic!("core-only execution invented an axiom");
            };
            assert_eq!(premises.iter().count(), rule.definition().body.len());
            for premise in premises.iter() {
                assert!(store.asserted.contains(premise) || store.inferred.contains(premise));
            }
        }
    }
}

#[tokio::test]
async fn joins_property_inheritance_domain_range_and_class_inheritance() {
    let mut store = Store::new([]);
    let subproperty = rdfs(&mut store, "subPropertyOf");
    let domain = rdfs(&mut store, "domain");
    let range = rdfs(&mut store, "range");
    store.asserted.extend([
        [11, subproperty, 12],
        [12, domain, 20],
        [12, range, 21],
        [20, SUBCLASS, 22],
        [10, 11, 13],
    ]);
    execute().run(&mut store).await.unwrap();
    for expected in [[10, 12, 13], [10, TYPE, 20], [10, TYPE, 22], [13, TYPE, 21]] {
        assert!(store.inferred.contains(&expected));
    }
    assert!(!store.inferred.contains(&[13, TYPE, 20]));
    assert!(!store.inferred.contains(&[10, TYPE, 21]));
}

#[tokio::test]
async fn disconnected_properties_do_not_create_domain_membership() {
    let mut store = Store::new([]);
    let domain = rdfs(&mut store, "domain");
    store.asserted.extend([[11, domain, 20], [10, 12, 13]]);
    execute().run(&mut store).await.unwrap();
    assert!(!store.inferred.contains(&[10, TYPE, 20]));
}

#[tokio::test]
async fn preserves_opaque_literal_ids_in_generalized_range_consequences() {
    let mut store = Store::new([]);
    let range = rdfs(&mut store, "range");
    let resource = rdfs(&mut store, "Resource");
    // Term 50 represents a caller-owned literal: execution must not drop it.
    store.asserted.extend([[11, range, 20], [10, 11, 50]]);
    execute().run(&mut store).await.unwrap();
    assert!(store.inferred.contains(&[50, TYPE, 20]));
    assert!(store.inferred.contains(&[50, TYPE, resource]));
    assert!(store.asserted.contains(&[10, 11, 50]));
}

#[test]
fn selects_only_supported_execution_contracts() {
    assert_eq!(
        "rdfs-core".parse::<ExecutionProfile>().unwrap(),
        ExecutionProfile::RdfsCore
    );
    assert_eq!(
        ExecutionProfile::RdfsSubclass.rules(),
        Preset::RdfsSubclass.rules()
    );
    assert!("rdfs".parse::<ExecutionProfile>().is_err());
    assert!("owl2-rl".parse::<ExecutionProfile>().is_err());
}
