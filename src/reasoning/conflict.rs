use super::{
    Limit, LiteralRole, ReasoningError, ReasoningStore, TripleCursor, TripleSource,
    plan::ConflictClause,
};
use rdf_reasoning::Rdf12DatatypeConflictId;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, future::Future, num::NonZeroU64};

/// Non-RDF evidence that one exact member belongs to two disjoint default types.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatatypeConflict<T> {
    /// Shared diagnostic identity; this is not a W3C entailment-rule name.
    pub id: Rdf12DatatypeConflictId,
    /// Exact caller-owned member key; no lexical/value identity is guessed.
    pub member: T,
    /// Both positive type premises in shared definition order.
    pub premises: [[T; 3]; 2],
}

/// Caller-owned asynchronous diagnostics storage, separate from RDF scratch.
pub trait DatatypeConflictSink: Send {
    /// Exact source term identity.
    type Term: Clone + Eq + Send + Sync;
    /// Backend failure; errors abort the attempt rather than lose diagnostics.
    type Error: Error + Send + Sync + 'static;
    /// Retain one witness with this attempt's immutable source scope and snapshot.
    /// Stream/spill records and discard partial output if the check fails.
    fn record(
        &mut self,
        conflict: DatatypeConflict<Self::Term>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// Strict caps on examined rows and emitted diagnostic records.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatatypeConflictLimits {
    diagnostics: NonZeroU64,
    matches: NonZeroU64,
}
impl DatatypeConflictLimits {
    /// Set diagnostic output and total cursor-row limits independently.
    pub const fn new(diagnostics: NonZeroU64, matches: NonZeroU64) -> Self {
        Self {
            diagnostics,
            matches,
        }
    }
}

/// Completed counters for the three selected checks; not a consistency verdict.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatatypeConflictReport {
    /// Examined rows, including repeated indexed probes across pairs.
    pub matches: u64,
    /// Recorded supported conflicts. Zero does not certify RDF/RDFS consistency.
    pub diagnostics: u64,
}

/// Failure to finish the entire selected source diagnostic phase.
#[derive(Debug)]
pub enum DatatypeConflictError<E> {
    /// Target augmentation must never participate in source consistency checks.
    WrongRole,
    /// Bounded diagnostic storage was exhausted; partial records are unpublished.
    DiagnosticsLimitExceeded,
    /// Compilation, backend, cancellation or cursor-work failure.
    Execution(ReasoningError<E>),
}
impl<E> From<ReasoningError<E>> for DatatypeConflictError<E> {
    fn from(error: ReasoningError<E>) -> Self {
        Self::Execution(error)
    }
}
impl<E: fmt::Display> fmt::Display for DatatypeConflictError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongRole => {
                f.write_str("datatype conflict checking requires source-only context")
            }
            Self::DiagnosticsLimitExceeded => {
                f.write_str("datatype conflict diagnostic limit exceeded")
            }
            Self::Execution(error) => error.fmt(f),
        }
    }
}
impl<E: Error + 'static> Error for DatatypeConflictError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Execution(error) => Some(error),
            _ => None,
        }
    }
}

/// Execute shared default-map conflict clauses against one source-only closure.
///
/// The controller must first validate source literals and close source facts,
/// including nonempty datatype witnesses, and must not add target context yet.
/// The role guard rejects explicit target input; it cannot audit a caller's
/// snapshot selection or certify completion of the preceding semantic phases.
/// This operation does not assert negative membership or write RDF facts.
pub struct Rdf12DatatypeConflictChecker {
    limits: DatatypeConflictLimits,
}
impl Rdf12DatatypeConflictChecker {
    /// Configure strict work and diagnostic limits.
    pub const fn new(limits: DatatypeConflictLimits) -> Self {
        Self { limits }
    }

    /// Stream all detected witnesses from one immutable selected source snapshot.
    ///
    /// At most two cursors and one fixed shared clause are retained. The sink
    /// owns scope/snapshot provenance and durable/spilled storage. An empty
    /// successful report only excludes these three witnessed default-map clashes
    /// in the supplied closure; it does not prove overall consistency.
    ///
    /// # Errors
    /// Wrong role, storage, cancellation and exhausted budgets abort the attempt.
    /// A late cursor failure must invalidate any already staged diagnostics.
    pub async fn run<S, D>(
        &self,
        store: &mut S,
        role: LiteralRole,
        sink: &mut D,
    ) -> Result<DatatypeConflictReport, DatatypeConflictError<S::Error>>
    where
        S: ReasoningStore,
        D: DatatypeConflictSink<Term = S::Term, Error = S::Error>,
    {
        if role != LiteralRole::Source {
            return Err(DatatypeConflictError::WrongRole);
        }
        Self::cancel(store)?;
        let source = store.snapshot().await.map_err(ReasoningError::Storage)?;
        let mut report = DatatypeConflictReport::default();
        for id in Rdf12DatatypeConflictId::ALL {
            let clause = ConflictClause::compile(*id, store).await?;
            let empty = std::array::from_fn(|_| None);
            let mut first = source
                .scan(clause.body[0].probe(&empty))
                .await
                .map_err(ReasoningError::Storage)?;
            while let Some(a) = self.next(&mut first, store, &mut report).await? {
                let Some(bindings) = clause.body[0].bind(&a, empty.clone()) else {
                    continue;
                };
                let mut second = source
                    .scan(clause.body[1].probe(&bindings))
                    .await
                    .map_err(ReasoningError::Storage)?;
                while let Some(b) = self.next(&mut second, store, &mut report).await? {
                    let Some(bindings) = clause.body[1].bind(&b, bindings.clone()) else {
                        continue;
                    };
                    let [Some(member)] = clause.head.probe(&bindings) else {
                        return Err(ReasoningError::UnsupportedDatatypeConflict(clause.id).into());
                    };
                    Self::cancel(store)?;
                    if report.diagnostics == self.limits.diagnostics.get() {
                        return Err(DatatypeConflictError::DiagnosticsLimitExceeded);
                    }
                    sink.record(DatatypeConflict {
                        id: clause.id,
                        member,
                        premises: [a.clone(), b],
                    })
                    .await
                    .map_err(ReasoningError::Storage)?;
                    report.diagnostics += 1;
                }
            }
        }
        Self::cancel(store)?;
        Ok(report)
    }
    async fn next<S: ReasoningStore, C: TripleCursor<Term = S::Term, Error = S::Error>>(
        &self,
        cursor: &mut C,
        store: &mut S,
        report: &mut DatatypeConflictReport,
    ) -> Result<Option<[S::Term; 3]>, ReasoningError<S::Error>> {
        Self::cancel(store)?;
        let row = cursor.next().await.map_err(ReasoningError::Storage)?;
        if row.is_some() {
            if report.matches == self.limits.matches.get() {
                return Err(ReasoningError::LimitExceeded(Limit::Matches));
            }
            report.matches += 1;
            crate::cooperative::checkpoint(report.matches).await;
            Self::cancel(store)?;
        }
        Ok(row)
    }
    fn cancel<S: ReasoningStore>(store: &S) -> Result<(), ReasoningError<S::Error>> {
        if store.is_cancelled() {
            Err(ReasoningError::Cancelled)
        } else {
            Ok(())
        }
    }
}
