mod cursor;
mod engine;
use super::{LiteralError, LiteralReader, OccurrencePath, ReasoningError};
pub use cursor::{LiteralCursorError, LiteralTermAccess, Rdf12LiteralOccurrences};
pub use engine::Rdf12LiteralTyper;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, future::Future, num::NonZeroU64};

/// Explicit semantic phase: selected source data or isolated target context.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LiteralRole {
    /// A literal appearing in the selected immutable source graph.
    Source,
    /// A literal in the target-relative entailment context, separate from source consistency.
    TargetContext,
}

/// One selected literal occurrence, including literals nested inside triple terms.
///
/// Root term keys retain their complete structure in the adapter's immutable
/// dictionary. Input snapshot and logical graph provenance remain caller-owned.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiteralOccurrence<T> {
    /// Exact literal term key; do not normalize or reconstruct its lexical form.
    pub literal: T,
    /// Root triple in which the literal occurs directly or recursively.
    pub root: [T; 3],
    /// Semantic phase owning this occurrence.
    pub role: LiteralRole,
    /// Exact direct or nested position within the selected root.
    pub path: OccurrencePath,
}

/// A fallible cursor step containing one occurrence and owned payload reader.
pub type LiteralOccurrenceRead<T, R, E> =
    Result<Option<(LiteralOccurrence<T>, R)>, LiteralCursorError<T, E>>;

/// Streaming capability over selected immutable literal occurrences and readers.
///
/// The adapter supplies every relevant direct/recursive occurrence for one phase.
/// Each reader must describe its occurrence's exact literal. Preserve selection,
/// blank scope, and root/graph provenance; never hydrate all literals or expose
/// a previous closure as new input. This is separate from RDF assertion scans:
/// an RDF predicate spelling cannot manufacture a semantic metadata premise.
pub trait LiteralOccurrenceCursor: Send {
    /// Exact term identities shared with inference scratch storage.
    type Term: Clone + Eq + Send + Sync;
    /// Cursor/read failure.
    type Error: Error + Send + Sync + 'static;
    /// Reader for one immutable literal, independent of scratch writes.
    /// It may borrow the immutable accessor for the cursor's lifetime.
    type Reader: LiteralReader<Error = Self::Error>;
    /// Return one occurrence and reader; zero remaining rows must mean successful EOF.
    fn next(
        &mut self,
    ) -> impl Future<Output = LiteralOccurrenceRead<Self::Term, Self::Reader, Self::Error>> + Send;
}

/// Aggregate resource budgets for one literal-typing phase.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiteralTypingLimits {
    pub(super) occurrences: NonZeroU64,
    pub(super) inferred: NonZeroU64,
    pub(super) bytes: u64,
}
impl LiteralTypingLimits {
    /// Bound examined occurrences, novel inferred tuples and total lexical bytes.
    /// A zero byte budget admits only empty payloads. Limits never truncate output.
    pub const fn new(occurrences: NonZeroU64, inferred: NonZeroU64, bytes: u64) -> Self {
        Self {
            occurrences,
            inferred,
            bytes,
        }
    }
}

/// Completed literal-typing counters, returned only after successful cursor EOF.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiteralTypingReport {
    /// Examined selected occurrences, including duplicate literal keys.
    pub occurrences: u64,
    /// Exact total lexical byte count, including opaque datatypes and duplicates.
    pub bytes: u64,
    /// Valid encountered literals outside the recognized default datatype map.
    pub opaque: u64,
    /// Novel generalized typing tuples; asserted/inferred duplicates are excluded.
    pub inferred: u64,
}

/// Aborted semantic phase; staged facts must remain unpublished.
#[derive(Debug)]
pub enum LiteralTypingError<T, E> {
    /// Cursor traversal/opening failed, retaining structural evidence when known.
    Cursor(LiteralCursorError<T, E>),
    /// Shared clause compilation, storage, cancellation or output-budget failure.
    Reasoning(ReasoningError<E>),
    /// Examination of another literal would exceed the occurrence budget.
    OccurrenceLimit,
    /// Cursor supplied an occurrence from a different semantic phase.
    RoleMismatch {
        /// Configured phase.
        expected: LiteralRole,
        /// Rejected occurrence, retained without reading its payload.
        occurrence: LiteralOccurrence<T>,
    },
    /// Literal inspection failed, retaining identity and source/target role.
    Literal {
        /// Exact selected occurrence whose inspection failed.
        occurrence: LiteralOccurrence<T>,
        /// Metadata, lexical, UTF-8, resource, cancellation or storage diagnostic.
        error: LiteralError<E>,
    },
}
impl<T, E: fmt::Display> fmt::Display for LiteralTypingError<T, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cursor(error) => error.fmt(f),
            Self::Reasoning(error) => error.fmt(f),
            Self::OccurrenceLimit => f.write_str("literal occurrence limit exceeded"),
            Self::RoleMismatch {
                expected,
                occurrence,
            } => write!(
                f,
                "expected {expected:?} literal role, received {:?}",
                occurrence.role
            ),
            Self::Literal { occurrence, error } => write!(
                f,
                "{:?} literal inspection failed: {error}",
                occurrence.role
            ),
        }
    }
}
impl<T: fmt::Debug, E: Error + 'static> Error for LiteralTypingError<T, E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Cursor(error) => error.source(),
            Self::Reasoning(error) => Some(error),
            Self::Literal { error, .. } => Some(error),
            _ => None,
        }
    }
}
