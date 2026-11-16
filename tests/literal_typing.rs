//! Metadata premises are validated capabilities, never user-asserted RDF relations.
#[allow(
    dead_code,
    reason = "shared capped graph fixture also serves core-rule tests"
)]
mod support;
use inference_engine::reasoning::{
    Evidence, Inference, LiteralError, LiteralOccurrence, LiteralOccurrenceCursor, LiteralReader,
    LiteralRole, LiteralTypingError, LiteralTypingLimits, Rdf12LiteralTyper,
};
use rdf_reasoning::{RDF12_LITERAL_DATATYPE_RELATION, Rdf12Datatype, Rdf12LiteralRuleId};
use rdf_structural_model::{BaseDirection, LanguageTag, Literal, LiteralMetadata};
use std::{
    collections::VecDeque,
    io,
    num::NonZeroU64,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use support::{Store, TYPE};

struct Reader {
    metadata: LiteralMetadata,
    payload: &'static [u8],
    position: usize,
    reads: Arc<AtomicUsize>,
    fail: bool,
}
impl LiteralReader for Reader {
    type Error = io::Error;
    fn metadata(&self) -> LiteralMetadata<&str> {
        self.metadata.as_ref()
    }
    fn is_cancelled(&self) -> bool {
        false
    }
    async fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        if self.fail {
            return Err(io::Error::other("literal read failure"));
        }
        let count = buffer.len().min(self.payload.len() - self.position);
        buffer[..count].copy_from_slice(&self.payload[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}
struct Cursor {
    rows: VecDeque<(LiteralOccurrence<u64>, Reader)>,
    fail_eof: bool,
}
impl Cursor {
    fn new(rows: impl IntoIterator<Item = (LiteralOccurrence<u64>, Reader)>) -> Self {
        // bounded: at most eight literal occurrences in these hand-authored fixtures.
        let mut result = Self {
            rows: VecDeque::new(),
            fail_eof: false,
        };
        for row in rows {
            assert!(result.rows.len() < 8);
            result.rows.push_back(row);
        }
        result
    }
}
impl LiteralOccurrenceCursor for Cursor {
    type Term = u64;
    type Error = io::Error;
    type Reader = Reader;
    async fn next(&mut self) -> io::Result<Option<(LiteralOccurrence<u64>, Reader)>> {
        if self.rows.is_empty() && self.fail_eof {
            return Err(io::Error::other("late occurrence failure"));
        }
        Ok(self.rows.pop_front())
    }
}
fn row(
    key: u64,
    literal: Literal,
    payload: &'static [u8],
    role: LiteralRole,
) -> (LiteralOccurrence<u64>, Reader) {
    (
        LiteralOccurrence {
            literal: key,
            root: [10, 11, key],
            role,
        },
        Reader {
            metadata: literal.metadata().clone(),
            payload,
            position: 0,
            reads: Arc::new(AtomicUsize::new(0)),
            fail: false,
        },
    )
}
fn typer(occurrences: u64, inferred: u64, bytes: u64, role: LiteralRole) -> Rdf12LiteralTyper {
    Rdf12LiteralTyper::new(
        LiteralTypingLimits::new(
            NonZeroU64::new(occurrences).unwrap(),
            NonZeroU64::new(inferred).unwrap(),
            bytes,
        ),
        role,
    )
}

#[tokio::test]
async fn executes_shared_typing_rules_with_metadata_evidence_and_exact_literal_keys() {
    let tag = LanguageTag::try_new("EN".to_owned()).unwrap();
    let mut rows = [
        row(50, Literal::string(""), b"text", LiteralRole::Source),
        row(
            51,
            Literal::language_tagged("", tag.clone()),
            b"\0",
            LiteralRole::Source,
        ),
        row(
            52,
            Literal::directional("", tag, BaseDirection::Rtl),
            b"\0",
            LiteralRole::Source,
        ),
    ];
    // A literal occurring inside an opaque nested triple term still has its root occurrence.
    rows[2].0.root = [10, 11, 300];
    let mut cursor = Cursor::new(rows);
    let mut store = Store::new([[10, 11, 50], [10, 11, 51], [10, 11, 300]]);
    let report = typer(3, 3, 6, LiteralRole::Source)
        .run(&mut store, &mut cursor)
        .await
        .unwrap();
    assert_eq!(
        (
            report.occurrences,
            report.inferred,
            report.bytes,
            report.opaque
        ),
        (3, 3, 6, 0)
    );
    for (key, datatype) in [
        (50, Rdf12Datatype::String),
        (51, Rdf12Datatype::LangString),
        (52, Rdf12Datatype::DirLangString),
    ] {
        let datatype_key = store.iri(datatype.iri());
        assert!(store.inferred.contains(&[key, TYPE, datatype_key]));
        let evidence = store.evidence.iter().find(|e| e.triple[0] == key).unwrap();
        let Evidence::Literal {
            rule,
            occurrence,
            lexical_bytes,
        } = &evidence.evidence
        else {
            panic!("invented RDF proof");
        };
        assert_eq!(*rule, Rdf12LiteralRuleId::GrdfD1(datatype));
        assert_eq!(occurrence.literal, key);
        assert_eq!(*lexical_bytes, if key == 50 { 4 } else { 1 });
        assert_eq!(
            serde_json::from_slice::<Inference<u64>>(&serde_json::to_vec(evidence).unwrap())
                .unwrap(),
            *evidence
        );
        assert_eq!(
            postcard::from_bytes::<Inference<u64>>(&postcard::to_allocvec(evidence).unwrap())
                .unwrap(),
            *evidence
        );
    }
}

#[tokio::test]
async fn rdf_relation_spelling_and_graph_datatype_declarations_cannot_forge_metadata() {
    let mut store = Store::new([]);
    let relation = store.iri(RDF12_LITERAL_DATATYPE_RELATION);
    let string = store.iri(Rdf12Datatype::String.iri());
    let unknown = store.iri("urn:unknown:datatype");
    let datatype = store.iri("http://www.w3.org/2000/01/rdf-schema#Datatype");
    store
        .asserted
        .extend([[50, relation, string], [unknown, TYPE, datatype]]);
    let mut opaque = row(
        51,
        Literal::string(""),
        b"not a number\0",
        LiteralRole::Source,
    );
    opaque.1.metadata =
        serde_json::from_str(r#"{"datatype":"urn:unknown:datatype","facets":"None"}"#).unwrap();
    let report = typer(2, 2, 100, LiteralRole::Source)
        .run(&mut store, &mut Cursor::new([opaque]))
        .await
        .unwrap();
    assert_eq!(
        (report.occurrences, report.opaque, report.inferred),
        (1, 1, 0)
    );
    assert!(store.inferred.is_empty());
}

#[tokio::test]
async fn preserves_phase_role_and_rejects_wrong_scope_before_payload_reads() {
    let mut occurrence = row(50, Literal::string(""), b"x", LiteralRole::TargetContext);
    occurrence.0.root = [70, 71, 50];
    let reads = occurrence.1.reads.clone();
    let mut store = Store::new([]);
    let error = typer(1, 1, 1, LiteralRole::Source)
        .run(&mut store, &mut Cursor::new([occurrence]))
        .await
        .unwrap_err();
    assert!(matches!(error, LiteralTypingError::RoleMismatch { .. }));
    assert_eq!(reads.load(Ordering::Relaxed), 0);
    assert!(store.inferred.is_empty());
    let occurrence = row(50, Literal::string(""), b"x", LiteralRole::TargetContext);
    typer(1, 1, 1, LiteralRole::TargetContext)
        .run(&mut store, &mut Cursor::new([occurrence]))
        .await
        .unwrap();
    assert!(
        matches!(&store.evidence[0].evidence, Evidence::Literal { occurrence, .. } if occurrence.role == LiteralRole::TargetContext)
    );
}

#[tokio::test]
async fn shares_total_byte_budget_and_handles_empty_literals_after_exact_exhaustion() {
    let mut store = Store::new([]);
    let rows = [
        row(50, Literal::string(""), b"x", LiteralRole::Source),
        row(51, Literal::string(""), b"", LiteralRole::Source),
    ];
    let report = typer(2, 2, 1, LiteralRole::Source)
        .run(&mut store, &mut Cursor::new(rows))
        .await
        .unwrap();
    assert_eq!((report.bytes, report.inferred), (1, 2));
    let rows = [
        row(50, Literal::string(""), b"x", LiteralRole::Source),
        row(51, Literal::string(""), b"y", LiteralRole::Source),
    ];
    let mut store = Store::new([]);
    let error = typer(2, 2, 1, LiteralRole::Source)
        .run(&mut store, &mut Cursor::new(rows))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        LiteralTypingError::Literal {
            error: LiteralError::ByteLimit,
            ..
        }
    ));
    assert_eq!(
        store.inferred.len(),
        1,
        "partial output remains unpublished staging"
    );
}

#[tokio::test]
async fn assertions_win_duplicates_and_occurrence_output_limits_abort_instead_of_truncating() {
    let mut store = Store::new([]);
    let string = store.iri(Rdf12Datatype::String.iri());
    store.asserted.insert([50, TYPE, string]);
    let rows = [
        row(50, Literal::string(""), b"x", LiteralRole::Source),
        row(51, Literal::string(""), b"y", LiteralRole::Source),
        row(51, Literal::string(""), b"y", LiteralRole::Source),
    ];
    let report = typer(3, 1, 3, LiteralRole::Source)
        .run(&mut store, &mut Cursor::new(rows))
        .await
        .unwrap();
    assert_eq!(report.inferred, 1);
    assert_eq!(store.evidence.len(), 1);
    for (occurrences, inferred) in [(1, 2), (2, 1)] {
        let rows = [
            row(50, Literal::string(""), b"x", LiteralRole::Source),
            row(51, Literal::string(""), b"y", LiteralRole::Source),
        ];
        let mut store = Store::new([]);
        assert!(
            typer(occurrences, inferred, 2, LiteralRole::Source)
                .run(&mut store, &mut Cursor::new(rows))
                .await
                .is_err()
        );
        assert_eq!(store.inferred.len(), 1);
    }
}

#[tokio::test]
async fn lexical_backend_and_late_cursor_failures_abort_with_role_and_literal_identity() {
    for payload in [b"\0".as_slice(), b"\xff".as_slice()] {
        let mut store = Store::new([]);
        let error = typer(1, 1, 10, LiteralRole::Source)
            .run(
                &mut store,
                &mut Cursor::new([row(50, Literal::string(""), payload, LiteralRole::Source)]),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(error, LiteralTypingError::Literal { occurrence, .. } if occurrence.literal == 50 && occurrence.role == LiteralRole::Source)
        );
        assert!(store.inferred.is_empty());
    }
    for fail_write in [false, true] {
        let mut cursor = Cursor::new([row(50, Literal::string(""), b"x", LiteralRole::Source)]);
        cursor.fail_eof = !fail_write;
        let mut store = Store::new([]);
        store.fail_write = fail_write;
        assert!(
            typer(1, 1, 1, LiteralRole::Source)
                .run(&mut store, &mut cursor)
                .await
                .is_err()
        );
        assert_eq!(store.inferred.len(), usize::from(!fail_write));
    }
}
