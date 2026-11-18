#[allow(dead_code, unused_imports)]
#[path = "../source_support/mod.rs"]
mod source_support;
mod syntax;
use inference_engine::{
    entailment::{EntailmentLimits, SimpleEntailment},
    reasoning::{
        ExecutionProfile, PropositionLimits, Rdf12TargetConstructor, Rdfs12Context,
        Rdfs12ContextOutcome, Reasoner, ReasoningStore, TargetConstructionLimits,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fmt::Write,
    io,
    num::{NonZeroU32, NonZeroU64},
    path::PathBuf,
};
use syntax::Parser;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum Requirement {
    RecursiveOccurrences,
    PropositionConstruction,
    ConsistentSimpleMatching,
    ContainerSeeding,
    TargetContext,
    DefaultDatatypeWitnesses,
    CoreRuleClosure,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u8,
    revision: u8,
    license: String,
    provenance: String,
    semantics: String,
    syntax: String,
    entailment: String,
    blank_node_scopes: String,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    source_turtle: String,
    target_turtle: String,
    expected: bool,
    requires: Vec<Requirement>,
}
#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum Stage {
    Syntax,
    Source,
    Context,
    Matching,
    Construction,
    Core,
}
#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum Missing {
    CompleteEntailmentDecision,
    BoundedPositiveSearch,
    SourceConflictPolicy,
}
#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum Status {
    Pass,
    Fail { expected: bool, observed: bool },
    Unsupported { missing: Missing },
    Error { stage: Stage, message: String },
}
#[derive(Serialize)]
pub struct ResultRow {
    id: String,
    status: Status,
}
#[derive(Serialize)]
pub struct Report {
    pub cases: Vec<ResultRow>,
}
impl Report {
    pub fn passed(&self) -> usize {
        self.cases
            .iter()
            .filter(|r| matches!(r.status, Status::Pass))
            .count()
    }
    pub fn unsupported(&self) -> usize {
        self.cases
            .iter()
            .filter(|r| matches!(r.status, Status::Unsupported { .. }))
            .count()
    }
    pub fn failed(&self) -> usize {
        self.cases
            .iter()
            .filter(|r| matches!(r.status, Status::Fail { .. }))
            .count()
    }
    pub fn errors(&self) -> usize {
        self.cases
            .iter()
            .filter(|r| matches!(r.status, Status::Error { .. }))
            .count()
    }
}
async fn manifest() -> Manifest {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/rdf12-authored/manifest.json");
    assert!(
        tokio::fs::metadata(&path)
            .await
            .expect("required authored manifest")
            .len()
            <= 65536
    );
    // bounded: immutable original manifest is capped at 64 KiB before and after reading.
    let bytes = tokio::fs::read(path)
        .await
        .expect("required authored manifest read");
    assert!(bytes.len() <= 65536);
    // bounded: a SHA-256 digest has 32 bytes and exactly 64 hexadecimal characters.
    let mut checksum = String::with_capacity(64);
    for byte in Sha256::digest(&bytes) {
        write!(checksum, "{byte:02x}").expect("writing hexadecimal bytes to String");
    }
    assert_eq!(
        checksum,
        "d415b4cc6fbae9129a431805df2ccd4c4472a49ae8a18af5d071b43efa363ecd"
    );
    let manifest: Manifest =
        serde_json::from_slice(&bytes).expect("strict published manifest schema");
    assert_eq!((manifest.format, manifest.revision), (1, 2));
    assert_eq!(manifest.cases.len(), 18);
    assert_eq!(manifest.license, "MIT OR Apache-2.0");
    assert_eq!(manifest.provenance, "authored-regression");
    assert_eq!(
        manifest.semantics,
        "https://www.w3.org/TR/2026/CRD-rdf12-semantics-20260924/"
    );
    assert_eq!(
        manifest.syntax,
        "https://www.w3.org/TR/2026/WD-rdf12-turtle-20261111/"
    );
    assert_eq!(manifest.entailment, "rdfs12-default");
    assert_eq!(manifest.blank_node_scopes, "independent-source-target");
    // bounded: exactly eighteen unique IDs in this pinned fixture revision.
    let mut ids = BTreeSet::new();
    for case in &manifest.cases {
        assert!(case.id.len() <= 128);
        assert!(ids.insert(case.id.clone()));
        assert!(case.requires.len() <= 8);
    }
    manifest
}
pub async fn run() -> Report {
    let manifest = manifest().await;
    // bounded: one status per eighteen immutable cases; graph state is discarded per case.
    let mut report = Report {
        cases: Vec::with_capacity(18),
    };
    for case in manifest.cases {
        let status = execute(&case).await;
        report.cases.push(ResultRow {
            id: case.id,
            status,
        });
    }
    report
}
async fn execute(case: &Case) -> Status {
    let mut parser = Parser::new();
    let source = match parser.graph(&case.source_turtle, 0).await {
        Ok(rows) => rows,
        Err(error) => return failure(Stage::Syntax, error),
    };
    let target = match parser.graph(&case.target_turtle, 1).await {
        Ok(rows) => rows,
        Err(error) => return failure(Stage::Syntax, error),
    };
    let target = match source_support::MemoryStore::new(target).snapshot().await {
        Ok(view) => view,
        Err(error) => return failure(Stage::Matching, error),
    };
    parser.fixture.store.inner.asserted.extend(source);
    let mut diagnostics = source_support::Diagnostics::default();
    let prepared = Rdfs12Context::new(source_support::context_config())
        .run(
            &mut parser.fixture.store,
            &parser.fixture.access,
            &target,
            &mut diagnostics,
        )
        .await;
    match prepared {
        Err(error) => {
            return Status::Error {
                stage: Stage::Context,
                message: error.to_string(),
            };
        }
        Ok(Rdfs12ContextOutcome::SourceConflicting(_)) => {
            return Status::Unsupported {
                missing: Missing::SourceConflictPolicy,
            };
        }
        Ok(Rdfs12ContextOutcome::Prepared { .. }) => {}
    }
    let matching = EntailmentLimits::new(
        NonZeroU32::new(128).unwrap(),
        NonZeroU32::new(128).unwrap(),
        NonZeroU32::new(256).unwrap(),
        NonZeroU64::new(2_000_000).unwrap(),
    );
    let matcher = SimpleEntailment::new(matching);
    let constructor = Rdf12TargetConstructor::new(TargetConstructionLimits::new(
        matching,
        PropositionLimits::new(
            NonZeroU64::new(3000).unwrap(),
            NonZeroU64::new(2_000_000).unwrap(),
        ),
        NonZeroU32::new(256).unwrap(),
        NonZeroU64::new(2_000_000).unwrap(),
    ));
    // bounded: at most four construction/core passes per immutable authored case.
    // Expected verdicts never select execution phases or candidate substitutions.
    for round in 0..=4 {
        let source = match parser.fixture.store.snapshot().await {
            Ok(view) => view,
            Err(error) => return failure(Stage::Source, error),
        };
        match matcher
            .check(&source, &target, &parser.fixture.access)
            .await
        {
            Err(error) => {
                return Status::Error {
                    stage: Stage::Matching,
                    message: error.to_string(),
                };
            }
            Ok(result) if result.entailed && case.expected => return Status::Pass,
            Ok(result) if result.entailed => {
                return Status::Fail {
                    expected: case.expected,
                    observed: true,
                };
            }
            Ok(_) => {}
        }
        if round == 4 {
            return Status::Unsupported {
                missing: Missing::BoundedPositiveSearch,
            };
        }
        let report = match constructor
            .run(&mut parser.fixture.store, &parser.fixture.access, &target)
            .await
        {
            Ok(report) => report,
            Err(error) => {
                return Status::Error {
                    stage: Stage::Construction,
                    message: error.to_string(),
                };
            }
        };
        if report.propositions.inferred == 0 {
            return Status::Unsupported {
                missing: Missing::CompleteEntailmentDecision,
            };
        }
        if let Err(error) = Reasoner::new(
            ExecutionProfile::RdfsCore,
            source_support::context_config().target.structural,
        )
        .run(&mut parser.fixture.store)
        .await
        {
            return Status::Error {
                stage: Stage::Core,
                message: error.to_string(),
            };
        }
    }
    unreachable!("bounded positive search always returns a verdict or an explicit limitation")
}
fn failure(stage: Stage, error: io::Error) -> Status {
    Status::Error {
        stage,
        message: error.to_string(),
    }
}
