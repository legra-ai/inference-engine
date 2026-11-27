//! Original W3C same-order queries execute through a real native SPARQL backend.

use inference_engine::shacl12::{Controller, Limits, Plan, Rule, RuleBackend, Work};
use oxigraph::{
    io::{RdfFormat, RdfParser},
    model::{GraphName, NamedNode, NamedOrBlankNode, Quad, Term, Triple},
    sparql::{BoundPreparedSparqlQuery, QueryResults, SparqlEvaluator},
    store::Store,
};
use sha2::{Digest, Sha256};
use shacl_structural_model::inference::RuleSchedule;
use std::{collections::HashSet, io, num::NonZeroU64, sync::Mutex};

const FIXTURE: &[u8] = include_bytes!("fixtures/shacl12-inference/same-order.ttl");
const PROVENANCE: &str = include_str!("fixtures/shacl12-inference/provenance.json");
const SH: &str = "http://www.w3.org/ns/shacl#";

type Failure = io::Error;
fn failure(error: impl std::fmt::Display) -> Failure {
    io::Error::other(error.to_string())
}

struct Backend {
    current: Store,
    pending: Store,
    queries: Vec<String>,
    inferred: HashSet<Triple>,
}
impl RuleBackend for Backend {
    type Rule = usize;
    // bounded: one frozen query handle per rule in this two-rule W3C fixture;
    // the database retains its own immutable read snapshot, not a graph copy.
    type Snapshot = Vec<Mutex<Option<BoundPreparedSparqlQuery<'static>>>>;
    type Error = Failure;

    async fn snapshot(&mut self) -> Result<Self::Snapshot, Failure> {
        self.queries
            .iter()
            .map(|query| {
                SparqlEvaluator::new()
                    .parse_query(query)
                    .map(|query| Mutex::new(Some(query.on_store(&self.current))))
                    .map_err(failure)
            })
            .collect()
    }
    async fn begin_layer(&mut self, _: &[Rule<Self::Rule>], _: &mut Work) -> Result<(), Failure> {
        Ok(())
    }
    async fn execute(
        &mut self,
        rule: &usize,
        view: &Self::Snapshot,
        _: u64,
        work: &mut Work,
    ) -> Result<(), Failure> {
        work.charge().map_err(failure)?;
        let query = view[*rule]
            .lock()
            .map_err(failure)?
            .take()
            .ok_or_else(|| failure("rule executed twice in one phase"))?;
        let QueryResults::Graph(triples) = query.execute().map_err(failure)? else {
            return Err(failure("expected CONSTRUCT"));
        };
        for triple in triples {
            work.charge().map_err(failure)?;
            let triple = triple.map_err(failure)?;
            self.pending
                .insert(&Quad::new(
                    triple.subject,
                    triple.predicate,
                    triple.object,
                    GraphName::DefaultGraph,
                ))
                .map_err(failure)?;
        }
        Ok(())
    }
    async fn merge_phase(&mut self, work: &mut Work) -> Result<u64, Failure> {
        let mut added = 0;
        for quad in self.pending.iter() {
            work.charge().map_err(failure)?;
            let quad = quad.map_err(failure)?;
            if !self.current.contains(&quad).map_err(failure)? {
                self.current.insert(&quad).map_err(failure)?;
                self.inferred
                    .insert(Triple::new(quad.subject, quad.predicate, quad.object));
                added += 1;
            }
        }
        self.pending.clear().map_err(failure)?;
        Ok(added)
    }
    async fn finish_layer(&mut self, _: &[Rule<Self::Rule>], _: &mut Work) -> Result<(), Failure> {
        Ok(())
    }
    async fn finish(&mut self, _: &mut Work) -> Result<(), Failure> {
        Ok(())
    }
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[tokio::test]
async fn original_same_order_construct_rules_infer_both_expected_triples() {
    let provenance: serde_json::Value = serde_json::from_str(PROVENANCE).unwrap();
    assert_eq!(
        Sha256::digest(FIXTURE)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        provenance["sha256"]
    );
    let base = Store::new().unwrap();
    base.load_from_reader(
        RdfParser::from_format(RdfFormat::Turtle)
            .with_base_iri("https://example.test/same-order.ttl")
            .unwrap(),
        FIXTURE,
    )
    .unwrap();
    // bounded: this original fixture contains exactly two sh:construct literals.
    let queries: Vec<_> = base
        .quads_for_pattern(
            None,
            Some(NamedNode::new(format!("{SH}construct")).unwrap().as_ref()),
            None,
            Some(GraphName::DefaultGraph.as_ref()),
        )
        .map(|quad| match quad.unwrap().object {
            Term::Literal(query) => {
                format!("PREFIX ex: <http://example.com/ns#>\n{}", query.value())
            }
            _ => panic!("nonliteral construct"),
        })
        .collect();
    assert_eq!(queries.len(), 2);
    let plan = Plan::try_new(
        (0..queries.len())
            .map(|id| Rule::new(id, RuleSchedule::new()))
            .collect(),
        2,
    )
    .unwrap();
    let before = base.len().unwrap();
    let mut backend = Backend {
        current: base,
        pending: Store::new().unwrap(),
        queries,
        inferred: HashSet::new(),
    };
    let report = Controller::new(Limits::new(
        8,
        NonZeroU64::new(100).unwrap(),
        NonZeroU64::new(10).unwrap(),
    ))
    .run(&plan, &mut backend)
    .await
    .unwrap();
    // bounded: exactly two three-term expected results in the original fixture.
    let expected = expected_triples(&backend.current);
    assert_eq!(expected.len(), 2);
    assert_eq!(backend.inferred, expected);
    assert_eq!(report.inferred, 2);
    assert_eq!(backend.current.len().unwrap(), before + 2);
}

fn object(store: &Store, subject: &Term, predicate: &str) -> Term {
    let subject = NamedOrBlankNode::try_from(subject.clone()).unwrap();
    let predicate = NamedNode::new(predicate).unwrap();
    let mut values = store.quads_for_pattern(
        Some(subject.as_ref()),
        Some(predicate.as_ref()),
        None,
        Some(GraphName::DefaultGraph.as_ref()),
    );
    let value = values.next().unwrap().unwrap().object;
    assert!(values.next().is_none());
    value
}
fn list(store: &Store, mut node: Term) -> Vec<Term> {
    const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    // bounded: this pinned result has at most three terms per list.
    let mut values = Vec::new();
    while node != NamedNode::new(format!("{RDF}nil")).unwrap().into() {
        assert!(values.len() < 3);
        values.push(object(store, &node, &format!("{RDF}first")));
        node = object(store, &node, &format!("{RDF}rest"));
    }
    values
}
fn expected_triples(store: &Store) -> HashSet<Triple> {
    let case = NamedNode::new("https://example.test/same-order")
        .unwrap()
        .into();
    let root = object(
        store,
        &case,
        "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#result",
    );
    list(store, root)
        .into_iter()
        .map(|node| {
            let [s, p, o]: [Term; 3] = list(store, node).try_into().unwrap();
            Triple::new(
                NamedOrBlankNode::try_from(s).unwrap(),
                NamedNode::try_from(p).unwrap(),
                o,
            )
        })
        .collect()
}
