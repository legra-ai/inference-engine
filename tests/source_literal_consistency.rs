//! Source semantic diagnostics require complete bounded inspection, before target work.
#[allow(dead_code)]
mod source_support;
use inference_engine::reasoning::{
    LiteralError, LiteralRole, LiteralTypingError, Rdf12SourceDiagnostic, Rdfs12Source,
    Rdfs12SourceError, Rdfs12SourceOutcome, SourceLiteralLimits, TermPosition,
};
use iri_rfc3987::Iri;
use rdf_structural_model::{BaseDirection, LanguageTag, Literal};
use source_support::{Diagnostics, Fixture, config};
use std::num::NonZeroU64;

fn n(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap()
}
fn source(f: &mut Fixture, payload: &str) -> [u64; 3] {
    let s = f.iri("urn:test:s");
    let p = f.iri("urn:test:p");
    let literal = f.literal(payload);
    let triple = [s, p, literal];
    f.store.inner.asserted.insert(triple);
    triple
}

#[tokio::test]
async fn nested_diagnostics_retain_original_root_paths_and_shared_exact_offsets() {
    let mut f = Fixture::new();
    let s = f.iri("urn:test:s");
    let p = f.iri("urn:test:p");
    let literal = f.literal("é\0\u{ffff}");
    let quoted = f.quoted([s, p, literal]).await;
    let root = [quoted, p, quoted];
    f.store.inner.asserted.insert(root);
    let mut diagnostics = Diagnostics::default();
    let result = Rdfs12Source::new(config())
        .run(&mut f.store, &f.access, &mut diagnostics)
        .await
        .unwrap();
    assert!(matches!(result, Rdfs12SourceOutcome::Conflicting(r)
        if r.diagnostics == 2 && r.literals == 2 && r.lexical_bytes == 12 && r.rounds == 0));
    for (value, root_position) in diagnostics
        .0
        .iter()
        .zip([TermPosition::Subject, TermPosition::Object])
    {
        let Rdf12SourceDiagnostic::IllTypedLiteral {
            occurrence,
            lexical_bytes,
            violation,
        } = value
        else {
            panic!("wrong diagnostic");
        };
        assert_eq!(occurrence.literal, literal);
        assert_eq!(occurrence.root, root);
        assert_eq!(occurrence.role, LiteralRole::Source);
        assert!(
            occurrence
                .path
                .positions()
                .copied()
                .eq([root_position, TermPosition::Object])
        );
        assert_eq!(*lexical_bytes, 6);
        assert_eq!(violation.fragment_offset + violation.error.byte_offset(), 2);
        assert_eq!(violation.error.character(), '\0');
        let json: Rdf12SourceDiagnostic<u64> =
            serde_json::from_str(&serde_json::to_string(value).unwrap()).unwrap();
        let binary: Rdf12SourceDiagnostic<u64> =
            postcard::from_bytes(&postcard::to_allocvec(value).unwrap()).unwrap();
        assert_eq!(value, &json);
        assert_eq!(value, &binary);
    }
    assert!(f.store.inner.inferred.is_empty());
}

#[tokio::test]
async fn late_source_cursor_failure_invalidates_already_staged_semantic_diagnostics() {
    let mut f = Fixture::new();
    source(&mut f, "\0");
    f.store.inner.fail_cursor = true; // Failure happens after all rows, never instead of a row.
    let mut diagnostics = Diagnostics::default();
    assert!(matches!(
        Rdfs12Source::new(config())
            .run(&mut f.store, &f.access, &mut diagnostics)
            .await,
        Err(Rdfs12SourceError::Literals(LiteralTypingError::Cursor(_)))
    ));
    assert_eq!(diagnostics.0.len(), 1); // Staged evidence is retained for debugging, not a completed result.
    assert!(f.store.inner.inferred.is_empty());
}

#[tokio::test]
async fn preflight_limits_and_sink_failure_are_errors_even_after_a_semantic_witness() {
    for failure in 0..4 {
        let mut f = Fixture::new();
        source(&mut f, "\0a");
        source(&mut f, "\0b");
        let mut limits = config();
        limits.inspection = match failure {
            0 => SourceLiteralLimits::new(n(1), n(10), 100),
            1 => SourceLiteralLimits::new(n(10), n(1), 100),
            2 => SourceLiteralLimits::new(n(10), n(10), 2),
            _ => SourceLiteralLimits::new(n(10), n(10), 100),
        };
        let mut diagnostics = Diagnostics(Vec::new(), failure == 3);
        let error = Rdfs12Source::new(limits)
            .run(&mut f.store, &f.access, &mut diagnostics)
            .await
            .unwrap_err();
        match failure {
            0 => assert!(matches!(
                error,
                Rdfs12SourceError::Literals(LiteralTypingError::OccurrenceLimit)
            )),
            1 => assert!(matches!(error, Rdfs12SourceError::DiagnosticLimit)),
            2 => assert!(matches!(
                error,
                Rdfs12SourceError::Literals(LiteralTypingError::Literal {
                    error: LiteralError::ByteLimit,
                    ..
                })
            )),
            _ => assert!(matches!(error, Rdfs12SourceError::Reasoning(_))),
        }
        assert!(f.store.inner.inferred.is_empty());
    }
}

#[tokio::test]
async fn language_directional_and_unrecognized_datatypes_are_not_ill_typed_strings() {
    let language = LanguageTag::try_new("EN-us".to_owned()).unwrap();
    for literal in [
        Literal::language_tagged("", language.clone()),
        Literal::directional("", language, BaseDirection::Rtl),
        Literal::typed("", Iri::parse("urn:unknown:datatype").unwrap().into()).unwrap(),
    ] {
        let mut f = Fixture::new();
        let s = f.iri("urn:test:s");
        let p = f.iri("urn:test:p");
        let key = f.literal_metadata("\0\u{fffe}\u{ffff}", literal.metadata().clone());
        f.store.inner.asserted.insert([s, p, key]);
        let mut diagnostics = Diagnostics::default();
        assert!(matches!(
            Rdfs12Source::new(config())
                .run(&mut f.store, &f.access, &mut diagnostics)
                .await
                .unwrap(),
            Rdfs12SourceOutcome::Prepared(_)
        ));
        assert!(diagnostics.0.is_empty());
    }
}
