//! Development-only independent Turtle syntax adapter with explicit fixture caps.
use super::source_support::Fixture;
use inference_engine::reasoning::Rdf12TripleTermStore;
use iri_rfc3987::Iri;
use oxrdf::{BaseDirection as Direction, Literal as SyntaxLiteral, Term, Triple};
use oxttl::TurtleParser;
use rdf_structural_model::{BaseDirection, LanguageTag, Literal};
use std::{collections::HashMap, future::Future, io, pin::Pin};

type TermRead<'a> = Pin<Box<dyn Future<Output = io::Result<u64>> + Send + 'a>>;
pub(super) struct Parser {
    pub fixture: Fixture,
    // bounded: at most sixteen literal identities per capped source/target fixture pair.
    literals: HashMap<SyntaxLiteral, u64>,
}
impl Parser {
    pub fn new() -> Self {
        Self {
            fixture: Fixture::new(),
            literals: HashMap::new(),
        }
    }
    pub async fn graph(&mut self, text: &str, scope: u8) -> io::Result<Vec<[u64; 3]>> {
        if text.len() > 16384 {
            return Err(io::Error::other("authored Turtle byte budget exceeded"));
        }
        // bounded: at most 128 parsed rows per 16 KiB fixture, enforced before each push.
        let mut rows = Vec::new();
        for triple in TurtleParser::new()
            .with_base_iri("https://github.com/legra-ai/rdf-reasoning/")
            .map_err(io::Error::other)?
            .for_slice(text.as_bytes())
        {
            if rows.len() == 128 {
                return Err(io::Error::other("authored fixture row budget exceeded"));
            }
            rows.push(
                self.triple(triple.map_err(io::Error::other)?, scope, 0)
                    .await?,
            );
        }
        Ok(rows)
    }
    fn term(&mut self, term: Term, scope: u8, depth: u32) -> TermRead<'_> {
        Box::pin(async move {
            // bounded: recursive syntax conversion retains at most 32 checked frames.
            if depth == 32 {
                return Err(io::Error::other("authored syntax nesting budget exceeded"));
            }
            match term {
                Term::NamedNode(iri) => Ok(self.fixture.iri(iri.as_str())),
                Term::BlankNode(blank) => self.fixture.blank(scope, blank.as_str()),
                Term::Literal(literal) => self.literal(literal),
                Term::Triple(triple) => {
                    let triple = self.triple(*triple, scope, depth + 1).await?;
                    self.fixture.store.resolve_triple_term(triple).await
                }
            }
        })
    }
    async fn triple(&mut self, triple: Triple, scope: u8, depth: u32) -> io::Result<[u64; 3]> {
        Ok([
            self.term(triple.subject.into(), scope, depth).await?,
            self.term(triple.predicate.into(), scope, depth).await?,
            self.term(triple.object, scope, depth).await?,
        ])
    }
    fn literal(&mut self, value: SyntaxLiteral) -> io::Result<u64> {
        if let Some(key) = self.literals.get(&value) {
            return Ok(*key);
        }
        if self.literals.len() == 16 {
            return Err(io::Error::other(
                "authored literal identity budget exceeded",
            ));
        }
        let metadata = if let Some(language) = value.language() {
            let tag = LanguageTag::try_new(language.to_owned()).map_err(io::Error::other)?;
            if let Some(direction) = value.direction() {
                Literal::directional(
                    "",
                    tag,
                    match direction {
                        Direction::Ltr => BaseDirection::Ltr,
                        Direction::Rtl => BaseDirection::Rtl,
                    },
                )
            } else {
                Literal::language_tagged("", tag)
            }
        } else {
            Literal::typed(
                "",
                Iri::parse(value.datatype().as_str())
                    .map_err(io::Error::other)?
                    .into(),
            )
            .map_err(io::Error::other)?
        };
        if value.value().len() > 32768 {
            return Err(io::Error::other("authored literal byte budget exceeded"));
        }
        let key = self
            .fixture
            .literal_metadata(value.value(), metadata.metadata().clone());
        self.literals.insert(value, key);
        Ok(key)
    }
}
