//! Literal metadata and lexical payloads are read independently with bounded memory.
use inference_engine::reasoning::{LiteralError, LiteralReader, Rdf12LiteralValidator};
use rdf_reasoning::Rdf12Datatype;
use rdf_structural_model::{BaseDirection, LanguageTag, Literal, LiteralMetadata, ModelError};
use std::{io, num::NonZeroU64};

struct Reader {
    metadata: LiteralMetadata,
    prefix: u64,
    tail: &'static [u8],
    position: u64,
    chunk: usize,
    fail_at: Option<u64>,
    cancel_at: Option<u64>,
    reads: usize,
    maximum_request: usize,
}

impl Reader {
    fn new(literal: Literal, tail: &'static [u8]) -> Self {
        Self {
            metadata: literal.metadata().clone(),
            prefix: 0,
            tail,
            position: 0,
            chunk: 4096,
            fail_at: None,
            cancel_at: None,
            reads: 0,
            maximum_request: 0,
        }
    }
}

impl LiteralReader for Reader {
    type Error = io::Error;
    fn metadata(&self) -> LiteralMetadata<&str> {
        self.metadata.as_ref()
    }
    fn is_cancelled(&self) -> bool {
        self.cancel_at.is_some_and(|n| self.position >= n)
    }
    async fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        self.maximum_request = self.maximum_request.max(output.len());
        if self.fail_at.is_some_and(|n| self.position >= n) {
            return Err(io::Error::other("payload read failed"));
        }
        let count = output
            .len()
            .min(self.chunk)
            .min((self.prefix + self.tail.len() as u64 - self.position) as usize);
        for byte in &mut output[..count] {
            *byte = if self.position < self.prefix {
                b'a'
            } else {
                self.tail[(self.position - self.prefix) as usize]
            };
            self.position += 1;
        }
        Ok(count)
    }
}

fn validator(maximum: u64) -> Rdf12LiteralValidator {
    Rdf12LiteralValidator::new(NonZeroU64::new(maximum).unwrap())
}

#[tokio::test]
async fn streams_multibyte_boundaries_and_uses_xml11_instead_of_xml10() {
    let lexical = "a\u{1}\u{d7ff}\u{e000}\u{fffd}\u{10000}\u{10ffff}";
    for chunk in 1..=9 {
        let mut reader = Reader::new(Literal::string(""), lexical.as_bytes());
        reader.chunk = chunk;
        let report = validator(lexical.len() as u64)
            .validate(&mut reader)
            .await
            .unwrap();
        assert_eq!(report.datatype, Some(Rdf12Datatype::String));
        assert_eq!(report.bytes, lexical.len() as u64);
    }
}

#[tokio::test]
async fn reports_exact_global_byte_offsets_after_streamed_megabytes() {
    for invalid in ["\0", "\u{fffe}", "\u{ffff}"] {
        let mut reader = Reader::new(Literal::string(""), invalid.as_bytes());
        reader.prefix = 5_000_003;
        reader.chunk = 1021;
        let error = validator(6_000_000)
            .validate(&mut reader)
            .await
            .unwrap_err();
        let LiteralError::Lexical {
            fragment_offset,
            error,
        } = error
        else {
            panic!("wrong diagnostic");
        };
        assert_eq!(fragment_offset + error.byte_offset(), reader.prefix);
        assert_eq!(error.character(), invalid.chars().next().unwrap());
        assert!(reader.maximum_request <= 4096);
        assert!(reader.reads > 1000);
    }
}

#[tokio::test]
async fn mandatory_language_datatypes_and_unknown_datatypes_preserve_lexical_bytes() {
    let language = LanguageTag::try_new("EN-us".to_owned()).unwrap();
    for (literal, datatype) in [
        (
            Literal::language_tagged("", language.clone()),
            Rdf12Datatype::LangString,
        ),
        (
            Literal::directional("", language.clone(), BaseDirection::Ltr),
            Rdf12Datatype::DirLangString,
        ),
        (
            Literal::directional("", language, BaseDirection::Rtl),
            Rdf12Datatype::DirLangString,
        ),
    ] {
        let mut reader = Reader::new(literal, "\0\u{fffe}\u{ffff}".as_bytes());
        let original = reader.metadata.clone();
        let report = validator(100).validate(&mut reader).await.unwrap();
        assert_eq!(report.datatype, Some(datatype));
        assert_eq!(reader.metadata, original);
    }
    let mut reader = Reader::new(Literal::string(""), b"not an integer\0");
    // Metadata transport can represent an arbitrary absolute datatype without interpreting it.
    reader.metadata =
        serde_json::from_str(r#"{"datatype":"urn:unknown:datatype","facets":"None"}"#).unwrap();
    let report = validator(100).validate(&mut reader).await.unwrap();
    assert_eq!(report.datatype, None);
    assert_eq!(report.bytes, 15);
}

#[tokio::test]
async fn invalid_standard_metadata_fails_before_opening_the_payload() {
    let mut reader = Reader::new(Literal::string(""), b"valid");
    reader.metadata = serde_json::from_str(
        r#"{"datatype":"http://www.w3.org/1999/02/22-rdf-syntax-ns#langString","facets":"None"}"#,
    )
    .unwrap();
    assert!(matches!(
        validator(100).validate(&mut reader).await,
        Err(LiteralError::Metadata(
            ModelError::InconsistentLiteralFacets
        ))
    ));
    assert_eq!(reader.reads, 0);
}

#[tokio::test]
async fn rejects_malformed_utf8_and_incomplete_final_scalars() {
    for (bytes, offset) in [
        (b"a\xff".as_slice(), 1),
        (b"a\xe2\x82".as_slice(), 1),
        (b"\xf0\x80\x80\x80".as_slice(), 0),
    ] {
        let mut reader = Reader::new(Literal::string(""), bytes);
        reader.chunk = 1;
        assert!(matches!(validator(100).validate(&mut reader).await,
            Err(LiteralError::Utf8 { byte_offset }) if byte_offset == offset));
    }
}

#[tokio::test]
async fn exact_byte_limit_can_prove_eof_but_over_limit_and_late_failures_abort() {
    let mut reader = Reader::new(Literal::string(""), b"abc");
    assert_eq!(validator(3).validate(&mut reader).await.unwrap().bytes, 3);
    let mut reader = Reader::new(Literal::string(""), b"abcd");
    assert!(matches!(
        validator(3).validate(&mut reader).await,
        Err(LiteralError::ByteLimit)
    ));
    let mut reader = Reader::new(Literal::string(""), b"");
    assert_eq!(validator(1).validate(&mut reader).await.unwrap().bytes, 0);
    for cancelled in [true, false] {
        let mut reader = Reader::new(Literal::string(""), b"abc");
        reader.chunk = 1;
        if cancelled {
            reader.cancel_at = Some(2);
        } else {
            reader.fail_at = Some(2);
        }
        let error = validator(10).validate(&mut reader).await.unwrap_err();
        assert!(matches!(
            (cancelled, error),
            (true, LiteralError::Cancelled) | (false, LiteralError::Storage(_))
        ));
    }
}
