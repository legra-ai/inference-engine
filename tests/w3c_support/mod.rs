mod graph;
use graph::Terms;
use inference_engine::entailment::{EntailmentLimits, SimpleEntailment};
use oxrdf::{NamedNode, NamedOrBlankNode, Term, Triple};
use oxttl::TurtleParser;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt::Write,
    num::{NonZeroU32, NonZeroU64},
    path::PathBuf,
};

const BASE: &str = "https://w3c.github.io/rdf-tests/rdf/rdf12/rdf-semantics/";
const MF: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    upstream_revision: String,
    sha256: BTreeMap<String, String>,
}
struct Fixtures {
    directory: PathBuf,
    provenance: Provenance,
}
impl Fixtures {
    async fn open() -> Self {
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rdf12-semantics");
        let bytes = Self::read(directory.join("provenance.json")).await;
        let provenance: Provenance =
            serde_json::from_slice(&bytes).expect("typed fixture provenance");
        assert_eq!(
            provenance.upstream_revision,
            "7b5afb91857a9c282c03d2e45d3f76485797c901"
        );
        assert_eq!(provenance.sha256.len(), 46);
        Self {
            directory,
            provenance,
        }
    }
    async fn read(path: PathBuf) -> Vec<u8> {
        // bounded: immutable checked-in fixtures are capped at 64 KiB per file.
        assert!(
            tokio::fs::metadata(&path)
                .await
                .expect("required fixture metadata")
                .len()
                <= 65_536
        );
        let bytes = tokio::fs::read(&path).await.expect("required fixture read");
        assert!(bytes.len() <= 65_536);
        bytes
    }
    async fn parse(&self, filename: &str) -> Vec<Triple> {
        assert!(!filename.contains('/') && !filename.contains('\\'));
        let bytes = Self::read(self.directory.join(filename)).await;
        // bounded: a SHA-256 digest has 32 bytes and exactly 64 hexadecimal characters.
        let mut checksum = String::with_capacity(64);
        for byte in Sha256::digest(&bytes) {
            write!(checksum, "{byte:02x}").expect("writing hexadecimal bytes to String");
        }
        assert_eq!(
            checksum,
            *self
                .provenance
                .sha256
                .get(filename)
                .expect("pinned fixture digest"),
            "fixture changed: {filename}"
        );
        // bounded: at most 1024 triples per immutable fixture/manifest, enforced per insertion.
        let mut triples = Vec::new();
        for triple in TurtleParser::new()
            .with_base_iri(BASE)
            .unwrap()
            .for_slice(&bytes)
        {
            assert!(triples.len() < 1024);
            triples.push(triple.unwrap_or_else(|error| panic!("fixture {filename}: {error}")));
        }
        triples
    }
}
fn named(value: &str) -> Term {
    NamedNode::new(value).unwrap().into()
}
fn object<'a>(triples: &'a [Triple], subject: &NamedOrBlankNode, predicate: &str) -> &'a Term {
    let mut values = triples
        .iter()
        .filter(|triple| &triple.subject == subject && triple.predicate.as_str() == predicate);
    let first = &values.next().expect("required manifest property").object;
    assert!(values.next().is_none(), "ambiguous manifest property");
    first
}
fn literal(term: &Term) -> &str {
    let Term::Literal(literal) = term else {
        panic!("expected manifest literal");
    };
    literal.value()
}
fn filename(term: &Term) -> &str {
    let Term::NamedNode(iri) = term else {
        panic!("expected fixture IRI");
    };
    iri.as_str()
        .strip_prefix(BASE)
        .expect("fixture must remain within the pinned suite")
}

pub struct SuiteReport {
    pub passed: usize,
    pub unsupported_datatypes: Vec<String>,
}
pub async fn run_simple() -> SuiteReport {
    let fixtures = Fixtures::open().await;
    let manifest = fixtures.parse("manifest.ttl").await;
    let manifest_type = named(&format!("{MF}Manifest"));
    let root = &manifest
        .iter()
        .find(|triple| {
            triple.predicate.as_str() == format!("{RDF}type") && triple.object == manifest_type
        })
        .expect("manifest root")
        .subject;
    let mut list = object(&manifest, root, &format!("{MF}entries")).clone();
    let nil = named(&format!("{RDF}nil"));
    let mut passed = 0;
    // bounded: the pinned manifest's entry list is capped at 128 before iteration.
    let mut unsupported_datatypes = Vec::new();
    let mut total = 0;
    while list != nil {
        assert!(total < 128, "bounded manifest entry list exhausted");
        total += 1;
        let subject = NamedOrBlankNode::try_from(list).expect("manifest list node");
        let entry =
            NamedOrBlankNode::try_from(object(&manifest, &subject, &format!("{RDF}first")).clone())
                .expect("manifest entry");
        list = object(&manifest, &subject, &format!("{RDF}rest")).clone();
        match literal(object(&manifest, &entry, &format!("{MF}entailmentRegime"))) {
            "simple" => {}
            "RDF" | "RDFS" => continue,
            other => panic!("unclassified entailment regime: {other}"),
        }
        let name = literal(object(&manifest, &entry, &format!("{MF}name")));
        if *object(&manifest, &entry, &format!("{MF}recognizedDatatypes")) != nil {
            eprintln!("UNSUPPORTED W3C simple: {name} requests additional datatype recognition");
            unsupported_datatypes.push(name.to_owned());
            continue;
        }
        let kind = object(&manifest, &entry, &format!("{RDF}type"));
        let expected = if *kind == named(&format!("{MF}PositiveEntailmentTest")) {
            true
        } else {
            assert_eq!(*kind, named(&format!("{MF}NegativeEntailmentTest")));
            false
        };
        let input = fixtures
            .parse(filename(object(&manifest, &entry, &format!("{MF}action"))))
            .await;
        let target = fixtures
            .parse(filename(object(&manifest, &entry, &format!("{MF}result"))))
            .await;
        let mut terms = Terms::default();
        let source = terms.graph(input, 0);
        let target = terms.graph(target, 1);
        let limits = EntailmentLimits::new(
            NonZeroU32::new(1024).unwrap(),
            NonZeroU32::new(4096).unwrap(),
            NonZeroU32::new(1024).unwrap(),
            NonZeroU64::new(1_000_000).unwrap(),
        );
        let report = SimpleEntailment::new(limits)
            .check(&source, &target, &terms)
            .await
            .unwrap_or_else(|error| panic!("W3C {name}: {error}"));
        assert_eq!(report.entailed, expected, "W3C {name}");
        eprintln!("PASS W3C simple: {name}");
        passed += 1;
    }
    SuiteReport {
        passed,
        unsupported_datatypes,
    }
}
