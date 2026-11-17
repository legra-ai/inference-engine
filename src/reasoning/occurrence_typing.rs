use super::{
    Evidence, Inference, Limit, LiteralRole, OccurrenceError, Rdf12OccurrenceCursor,
    ReasoningError, ReasoningStore, TripleCursor, plan::OccurrenceClause,
};
use crate::entailment::EntailmentTerms;
use rdf_reasoning::Rdf12OccurrenceRuleId;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, num::NonZeroU64};

/// Completed counters for one immutable occurrence phase; not a closure claim.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccurrenceTypingReport {
    /// Actual appearances inspected, including repeated keys at distinct paths.
    pub occurrences: u64,
    /// Novel RDF typing conclusions, excluding asserted/inferred duplicates.
    pub inferred: u64,
}

/// Failure of the complete occurrence-typing phase; discard partial staging.
#[derive(Debug)]
pub enum OccurrenceTypingError<E> {
    /// Explicit cursor role does not match the selected execution role.
    RoleMismatch {
        /// Phase's configured role.
        expected: LiteralRole,
        /// Cursor's configured role.
        actual: LiteralRole,
    },
    /// Root/decomposition/cancellation or traversal budget failed.
    Traversal(OccurrenceError<E>),
    /// Shared compilation, store, cancellation or output budget failed.
    Reasoning(ReasoningError<E>),
}
impl<E> From<ReasoningError<E>> for OccurrenceTypingError<E> {
    fn from(error: ReasoningError<E>) -> Self {
        Self::Reasoning(error)
    }
}
impl<E: fmt::Display> fmt::Display for OccurrenceTypingError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RoleMismatch { expected, actual } => write!(
                f,
                "occurrence role mismatch: expected {expected:?}, got {actual:?}"
            ),
            Self::Traversal(error) => error.fmt(f),
            Self::Reasoning(error) => error.fmt(f),
        }
    }
}
impl<E: Error + 'static> Error for OccurrenceTypingError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Traversal(error) => Some(error),
            Self::Reasoning(error) => Some(error),
            _ => None,
        }
    }
}

/// Execute shared rdfD2/rdfs4 over authoritative recursive appearances.
///
/// Source admits both clauses. Target context admits rdfs4 only, never predicate
/// typing. The caller selects source/target snapshots and schedules source
/// consistency before target augmentation. This phase does not close newly staged
/// triples recursively, inspect lexical payloads or construct proposition terms.
pub struct Rdf12OccurrenceTyper {
    inferred: NonZeroU64,
    role: LiteralRole,
}
impl Rdf12OccurrenceTyper {
    /// Select strict novel-output cap and one explicit source/target-context role.
    /// The cursor independently bounds traversal, depth and inspected work.
    pub const fn new(inferred: NonZeroU64, role: LiteralRole) -> Self {
        Self { inferred, role }
    }

    /// Stage typing facts with exact structural occurrence evidence.
    ///
    /// The cursor role is checked before any source read or compilation. RDF
    /// assertions spelling an auxiliary relation cannot bind that capability.
    /// Nested parents remain structural evidence, never asserted RDF premises.
    /// Keep scope/snapshot/source-graph origin alongside every retained proof.
    ///
    /// # Errors
    /// Role mismatch, traversal, compilation, cancellation, storage and output
    /// budget failures invalidate all partial output for the enclosing attempt.
    pub async fn run<S, A, C>(
        &self,
        store: &mut S,
        cursor: &mut Rdf12OccurrenceCursor<'_, A, C>,
    ) -> Result<OccurrenceTypingReport, OccurrenceTypingError<S::Error>>
    where
        S: ReasoningStore,
        A: EntailmentTerms<Term = S::Term, Error = S::Error>,
        C: TripleCursor<Term = S::Term, Error = S::Error>,
    {
        if cursor.role() != self.role {
            return Err(OccurrenceTypingError::RoleMismatch {
                expected: self.role,
                actual: cursor.role(),
            });
        }
        Self::cancel(store)?;
        // bounded: source selects two clauses; target context selects one.
        let mut clauses = Vec::with_capacity(2);
        for &id in Rdf12OccurrenceRuleId::ALL {
            if self.role == LiteralRole::TargetContext && id == Rdf12OccurrenceRuleId::RdfD2 {
                continue;
            }
            clauses.push(OccurrenceClause::compile(id, store).await?);
        }
        let mut report = OccurrenceTypingReport::default();
        loop {
            Self::cancel(store)?;
            let next = cursor
                .next()
                .await
                .map_err(OccurrenceTypingError::Traversal)?;
            Self::cancel(store)?;
            let Some(occurrence) = next else {
                return Ok(report);
            };
            report.occurrences += 1;
            for clause in &clauses {
                let Some(bindings) = clause.bind(&occurrence) else {
                    continue;
                };
                let triple = clause
                    .head
                    .ground(&bindings)
                    .ok_or(ReasoningError::UnsupportedOccurrenceRule(clause.id))?;
                if store
                    .contains(&triple)
                    .await
                    .map_err(ReasoningError::Storage)?
                {
                    continue;
                }
                Self::cancel(store)?;
                if report.inferred == self.inferred.get() {
                    return Err(ReasoningError::LimitExceeded(Limit::Inferred).into());
                }
                if store
                    .insert_inferred(Inference {
                        triple,
                        evidence: Evidence::Occurrence {
                            rule: clause.id,
                            occurrence: occurrence.clone(),
                        },
                    })
                    .await
                    .map_err(ReasoningError::Storage)?
                {
                    report.inferred += 1;
                }
            }
        }
    }
    fn cancel<S: ReasoningStore>(store: &S) -> Result<(), ReasoningError<S::Error>> {
        if store.is_cancelled() {
            Err(ReasoningError::Cancelled)
        } else {
            Ok(())
        }
    }
}
