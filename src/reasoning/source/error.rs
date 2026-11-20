use super::super::{
    DatatypeConflictError, LiteralTypingError, OccurrenceError, OccurrenceTypingError,
    ReasoningError, TermOccurrence,
};
use std::{error::Error, fmt};

/// Source preparation failed; all partial scratch output must remain unpublished.
#[derive(Debug)]
pub enum Rdfs12SourceError<T, E> {
    /// Shared axiom, core, constructor, storage or cancellation failure.
    Reasoning(ReasoningError<E>),
    /// Selected authoritative structural traversal failed.
    Traversal(OccurrenceError<E>),
    /// Shared source occurrence typing failed.
    Occurrences(OccurrenceTypingError<E>),
    /// Source literal metadata/payload/typing failed, retaining source role and path.
    Literals(LiteralTypingError<T, E>),
    /// Authoritative container classification failed for this exact appearance.
    Vocabulary {
        /// Selected root, path and exact term identity.
        occurrence: TermOccurrence<T>,
        /// Backend/classification/resource error.
        error: E,
    },
    /// Backend returned a descriptor exceeding the explicit metadata cap.
    VocabularyLimit,
    /// Checked descriptor did not resolve to the selected appearing IRI identity.
    VocabularyIdentity,
    /// Existing triple-term decomposition did not resolve to the same exact key.
    PropositionIdentity,
    /// Complete source-only diagnostic scan failed.
    Conflicts(DatatypeConflictError<E>),
    /// Source literal semantic-diagnostic budget was exhausted.
    DiagnosticLimit,
    /// Outer round budget ended before proving the selected source fixpoint.
    Rounds,
    /// Counters cannot represent total bounded work; no wrapped report was returned.
    CounterOverflow,
}
impl<T, E> From<ReasoningError<E>> for Rdfs12SourceError<T, E> {
    fn from(error: ReasoningError<E>) -> Self {
        Self::Reasoning(error)
    }
}
impl<T, E: fmt::Display> fmt::Display for Rdfs12SourceError<T, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reasoning(error) => error.fmt(f),
            Self::Traversal(error) => error.fmt(f),
            Self::Occurrences(error) => error.fmt(f),
            Self::Literals(error) => error.fmt(f),
            Self::Vocabulary { error, .. } => {
                write!(f, "source container classification failed: {error}")
            }
            Self::VocabularyLimit => f.write_str("source container metadata limit exceeded"),
            Self::VocabularyIdentity => {
                f.write_str("source container descriptor does not match selected term")
            }
            Self::PropositionIdentity => {
                f.write_str("source triple-term construction does not match selected term")
            }
            Self::Conflicts(error) => error.fmt(f),
            Self::DiagnosticLimit => f.write_str("source literal diagnostic limit exceeded"),
            Self::Rounds => f.write_str("source preparation round limit exceeded"),
            Self::CounterOverflow => f.write_str("source preparation counter capacity exceeded"),
        }
    }
}
impl<T: fmt::Debug, E: Error + 'static> Error for Rdfs12SourceError<T, E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Reasoning(error) => Some(error),
            Self::Traversal(error) => Some(error),
            Self::Occurrences(error) => Some(error),
            Self::Literals(error) => error.source(),
            Self::Vocabulary { error, .. } => Some(error),
            Self::Conflicts(error) => Some(error),
            _ => None,
        }
    }
}
