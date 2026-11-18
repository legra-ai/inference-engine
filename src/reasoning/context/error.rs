use super::super::{
    LiteralTypingError, OccurrenceError, OccurrenceTypingError, Rdfs12SourceError, ReasoningError,
    TermOccurrence,
};
use std::{error::Error, fmt};

/// Failed source/context attempt; discard all partially staged context output.
#[derive(Debug)]
pub enum Rdfs12ContextError<T, E> {
    /// Source preparation or its complete supported diagnostic phase failed.
    Source(Rdfs12SourceError<T, E>),
    /// Target seeding/core compilation, storage, cancellation or output budget failed.
    Reasoning(ReasoningError<E>),
    /// Original target traversal, decomposition or traversal budget failed.
    Traversal(OccurrenceError<E>),
    /// Target resource-typing phase failed.
    Occurrences(OccurrenceTypingError<E>),
    /// Target literal inspection failed, preserving its role, root and path.
    Literals(LiteralTypingError<T, E>),
    /// Authoritative container metadata lookup failed for this target appearance.
    Vocabulary {
        /// Exact selected original target root, path and term.
        occurrence: TermOccurrence<T>,
        /// Backend metadata/access/resource error.
        error: E,
    },
    /// Target container descriptor exceeded the explicit metadata cap.
    VocabularyLimit,
    /// Target container descriptor did not resolve to the selected exact term key.
    VocabularyIdentity,
    /// Total counters exceeded representable capacity; no success was returned.
    CounterOverflow,
}
impl<T, E> From<ReasoningError<E>> for Rdfs12ContextError<T, E> {
    fn from(error: ReasoningError<E>) -> Self {
        Self::Reasoning(error)
    }
}
impl<T, E: fmt::Display> fmt::Display for Rdfs12ContextError<T, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => error.fmt(f),
            Self::Reasoning(error) => error.fmt(f),
            Self::Traversal(error) => error.fmt(f),
            Self::Occurrences(error) => error.fmt(f),
            Self::Literals(error) => error.fmt(f),
            Self::Vocabulary { error, .. } => {
                write!(f, "target container classification failed: {error}")
            }
            Self::VocabularyLimit => f.write_str("target container metadata limit exceeded"),
            Self::VocabularyIdentity => {
                f.write_str("target container descriptor does not match selected term")
            }
            Self::CounterOverflow => f.write_str("target-context counter capacity exceeded"),
        }
    }
}
impl<T: fmt::Debug, E: Error + 'static> Error for Rdfs12ContextError<T, E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Source(error) => error.source(),
            Self::Reasoning(error) => Some(error),
            Self::Traversal(error) => Some(error),
            Self::Occurrences(error) => Some(error),
            Self::Literals(error) => error.source(),
            Self::Vocabulary { error, .. } => Some(error),
            _ => None,
        }
    }
}
