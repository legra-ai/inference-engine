use super::{
    LiteralOccurrence, LiteralOccurrenceCursor, LiteralOccurrenceRead, LiteralReader, LiteralRole,
};
use crate::{
    entailment::EntailmentTerms,
    reasoning::{
        OccurrenceError, OccurrenceLimits, Rdf12OccurrenceCursor, TermOccurrence, TripleCursor,
    },
};
use std::{error::Error, fmt, future::Future};

/// Asynchronous access to one exact literal's metadata and streamed payload.
///
/// Reuse the immutable identity/structural accessor used by occurrence traversal.
/// Reader payloads may borrow it or use independent backend handles; never clone
/// a whole lexical form or hydrate all literals. Preserve exact language/direction
/// identity and reject unsupported term extensions at the admission boundary.
pub trait LiteralTermAccess: EntailmentTerms {
    /// Reader may borrow metadata/payload handles for the immutable accessor lifetime.
    type Reader<'a>: LiteralReader<Error = Self::Error>
    where
        Self: 'a;
    /// Return a reader for a literal and `None` only for a supported nonliteral.
    /// Cache misses must load authoritative classification/metadata or fail;
    /// they must never become `None`. Opening must not buffer the lexical value.
    fn open_literal<'a>(
        &'a self,
        term: &Self::Term,
    ) -> impl Future<Output = Result<Option<Self::Reader<'a>>, Self::Error>> + Send;
}

/// Failure to enumerate or open a selected literal; never successful EOF.
#[derive(Debug)]
pub enum LiteralCursorError<T, E> {
    /// Custom cursor/backend selection failed.
    Storage(E),
    /// Native recursive occurrence traversal failed.
    Traversal(OccurrenceError<E>),
    /// Opening a known occurrence failed, retaining its exact structural context.
    Open {
        /// Actual term/root/parent/path/role whose literal access failed.
        occurrence: TermOccurrence<T>,
        /// Backend opening failure, before any lexical read.
        error: E,
    },
}
impl<T, E> From<E> for LiteralCursorError<T, E> {
    fn from(error: E) -> Self {
        Self::Storage(error)
    }
}
impl<T, E: fmt::Display> fmt::Display for LiteralCursorError<T, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(f, "literal occurrence access failed: {error}"),
            Self::Traversal(error) => error.fmt(f),
            Self::Open { error, .. } => write!(f, "literal opening failed: {error}"),
        }
    }
}
impl<T: fmt::Debug, E: Error + 'static> Error for LiteralCursorError<T, E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) | Self::Open { error, .. } => Some(error),
            Self::Traversal(error) => Some(error),
        }
    }
}

/// Native bounded recursive literal selection, with readers borrowed from storage.
///
/// Walks actual selected term structure using `Rdf12OccurrenceCursor`, then opens
/// supported literal readers one at a time. Repeated keys keep distinct paths;
/// metadata datatype IRIs are never traversed as actual RDF terms. No input graph,
/// dictionary or whole literal value is collected. Scratch writes must not change
/// the selected immutable root/dictionary/payload view during this phase.
pub struct Rdf12LiteralOccurrences<'a, A: LiteralTermAccess, C: TripleCursor> {
    cursor: Rdf12OccurrenceCursor<'a, A, C>,
    access: &'a A,
}
impl<'a, A, C> Rdf12LiteralOccurrences<'a, A, C>
where
    A: LiteralTermAccess,
    C: TripleCursor<Term = A::Term, Error = A::Error>,
{
    /// Select one immutable source/target scope and explicit traversal budgets.
    /// Retain snapshot/source-graph provenance alongside emitted root/path records.
    pub fn new(roots: C, access: &'a A, role: LiteralRole, limits: OccurrenceLimits) -> Self {
        Self {
            cursor: Rdf12OccurrenceCursor::new(roots, access, role, limits),
            access,
        }
    }
}
impl<'a, A, C> LiteralOccurrenceCursor for Rdf12LiteralOccurrences<'a, A, C>
where
    A: LiteralTermAccess,
    C: TripleCursor<Term = A::Term, Error = A::Error>,
{
    type Term = A::Term;
    type Error = A::Error;
    type Reader = A::Reader<'a>;
    async fn next(&mut self) -> LiteralOccurrenceRead<Self::Term, Self::Reader, Self::Error> {
        loop {
            let Some(appearance) = self
                .cursor
                .next()
                .await
                .map_err(LiteralCursorError::Traversal)?
            else {
                return Ok(None);
            };
            let reader = self
                .access
                .open_literal(&appearance.term)
                .await
                .map_err(|error| LiteralCursorError::Open {
                    occurrence: appearance.clone(),
                    error,
                })?;
            let Some(reader) = reader else {
                continue;
            };
            return Ok(Some((
                LiteralOccurrence {
                    literal: appearance.term,
                    root: appearance.root,
                    role: appearance.role,
                    path: appearance.path,
                },
                reader,
            )));
        }
    }
}
