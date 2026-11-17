//! Native literal occurrence traversal borrows payloads and retains exact paths.
#[allow(dead_code)]
mod support;
use inference_engine::{
    entailment::EntailmentTerms,
    reasoning::{
        Evidence, LiteralCursorError, LiteralOccurrenceCursor, LiteralReader, LiteralRole,
        LiteralTermAccess, LiteralTypingError, LiteralTypingLimits, OccurrenceLimits,
        Rdf12LiteralOccurrences, Rdf12LiteralTyper, ReasoningStore, TermPosition as P,
        TripleSource,
    },
};
use rdf_structural_model::{Literal, LiteralMetadata};
use std::{
    io,
    num::{NonZeroU32, NonZeroU64},
    sync::atomic::{AtomicUsize, Ordering},
};
use support::{Store, TYPE};
struct Access {
    payload: String,
    metadata: LiteralMetadata,
    reads: AtomicUsize,
    fail_open: bool,
    fail_structure: bool,
}
struct Reader<'a> {
    payload: &'a [u8],
    metadata: &'a LiteralMetadata,
    reads: &'a AtomicUsize,
    offset: usize,
}
impl LiteralReader for Reader<'_> {
    type Error = io::Error;
    fn metadata(&self) -> LiteralMetadata<&str> {
        self.metadata.as_ref()
    }
    async fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        let size = out.len().min(self.payload.len() - self.offset);
        out[..size].copy_from_slice(&self.payload[self.offset..self.offset + size]);
        self.offset += size;
        Ok(size)
    }
    fn is_cancelled(&self) -> bool {
        false
    }
}
impl EntailmentTerms for Access {
    type Term = u64;
    type Error = io::Error;
    async fn is_blank(&self, _: &u64) -> io::Result<bool> {
        Ok(false)
    }
    async fn triple_term(&self, key: &u64) -> io::Result<Option<[u64; 3]>> {
        if *key == 90 && self.fail_structure {
            return Err(io::Error::other("structure load failed"));
        }
        Ok(if *key == 90 { Some([30, 11, 30]) } else { None })
    }
    fn is_cancelled(&self) -> bool {
        false
    }
}
impl LiteralTermAccess for Access {
    type Reader<'a> = Reader<'a>;
    async fn open_literal<'a>(&'a self, key: &u64) -> io::Result<Option<Reader<'a>>> {
        if *key != 30 {
            return Ok(None);
        }
        if self.fail_open {
            return Err(io::Error::other("literal open failed"));
        }
        Ok(Some(Reader {
            payload: self.payload.as_bytes(),
            metadata: &self.metadata,
            reads: &self.reads,
            offset: 0,
        }))
    }
}
fn access() -> Access {
    // bounded: one 9000-byte fixture payload, borrowed rather than copied by readers.
    Access {
        payload: "a".repeat(9000),
        metadata: Literal::string("").metadata().clone(),
        reads: AtomicUsize::new(0),
        fail_open: false,
        fail_structure: false,
    }
}
fn n(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap()
}
fn traversal() -> OccurrenceLimits {
    OccurrenceLimits::new(NonZeroU32::new(3).unwrap(), n(100), n(1000))
}
#[tokio::test]
async fn borrows_nested_literal_readers_and_preserves_duplicate_paths_in_typing_evidence() {
    let access = access();
    let mut store = Store::new([[7, 8, 90], [9, 8, 30]]);
    let roots = store
        .snapshot()
        .await
        .unwrap()
        .scan([None; 3])
        .await
        .unwrap();
    let mut cursor = Rdf12LiteralOccurrences::new(roots, &access, LiteralRole::Source, traversal());
    let report = Rdf12LiteralTyper::new(
        LiteralTypingLimits::new(n(3), n(1), 27000),
        LiteralRole::Source,
    )
    .run(&mut store, &mut cursor)
    .await
    .unwrap();
    assert_eq!(report.occurrences, 3);
    assert_eq!(report.bytes, 27000);
    assert_eq!(report.inferred, 1);
    assert!(access.reads.load(Ordering::Relaxed) >= 12);
    let Evidence::Literal { occurrence, .. } = &store.evidence[0].evidence else {
        panic!("missing literal evidence")
    };
    assert_eq!(occurrence.root, [7, 8, 90]);
    assert_eq!(occurrence.literal, 30);
    assert_eq!(
        occurrence.path.positions().copied().collect::<Vec<_>>(),
        [P::Object, P::Subject]
    );
    let string = store.iri("http://www.w3.org/2001/XMLSchema#string");
    assert!(store.inferred.contains(&[30, TYPE, string]));
    assert!(!store.inferred.contains(&[30, 11, 30]));
    let roots = store
        .snapshot()
        .await
        .unwrap()
        .scan([Some(7), Some(8), Some(90)])
        .await
        .unwrap();
    let mut cursor =
        Rdf12LiteralOccurrences::new(roots, &access, LiteralRole::TargetContext, traversal());
    for tail in [P::Subject, P::Object] {
        let (occurrence, reader) = cursor.next().await.unwrap().unwrap();
        assert_eq!(occurrence.role, LiteralRole::TargetContext);
        assert_eq!(
            occurrence.path.positions().copied().collect::<Vec<_>>(),
            [P::Object, tail]
        );
        assert_eq!(reader.payload.as_ptr(), access.payload.as_ptr());
    }
    assert!(cursor.next().await.unwrap().is_none());
}
#[tokio::test]
async fn opening_failure_retains_actual_root_path_and_role() {
    let mut access = access();
    access.fail_open = true;
    let mut store = Store::new([[7, 8, 90]]);
    let roots = store
        .snapshot()
        .await
        .unwrap()
        .scan([None; 3])
        .await
        .unwrap();
    let mut cursor = Rdf12LiteralOccurrences::new(roots, &access, LiteralRole::Source, traversal());
    let error = Rdf12LiteralTyper::new(
        LiteralTypingLimits::new(n(3), n(1), 27000),
        LiteralRole::Source,
    )
    .run(&mut store, &mut cursor)
    .await
    .unwrap_err();
    let LiteralTypingError::Cursor(LiteralCursorError::Open { occurrence, .. }) = error else {
        panic!("lost opening evidence")
    };
    assert_eq!(occurrence.term, 30);
    assert_eq!(occurrence.root, [7, 8, 90]);
    assert_eq!(
        occurrence.path.positions().copied().collect::<Vec<_>>(),
        [P::Object, P::Subject]
    );
    assert!(store.inferred.is_empty());
    assert_eq!(access.reads.load(Ordering::Relaxed), 0);
}
#[tokio::test]
async fn structural_and_late_root_failures_are_not_successful_literal_eof() {
    for structure in [false, true] {
        let mut access = access();
        access.fail_structure = structure;
        let mut store = Store::new([[7, 8, 90]]);
        store.fail_cursor = !structure;
        let roots = store
            .snapshot()
            .await
            .unwrap()
            .scan([None; 3])
            .await
            .unwrap();
        let mut cursor =
            Rdf12LiteralOccurrences::new(roots, &access, LiteralRole::Source, traversal());
        assert!(matches!(
            Rdf12LiteralTyper::new(
                LiteralTypingLimits::new(n(3), n(1), 27000),
                LiteralRole::Source
            )
            .run(&mut store, &mut cursor)
            .await,
            Err(LiteralTypingError::Cursor(LiteralCursorError::Traversal(_)))
        ));
    }
}
