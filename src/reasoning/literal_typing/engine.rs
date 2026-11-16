use super::{
    LiteralOccurrenceCursor, LiteralRole, LiteralTypingError, LiteralTypingLimits,
    LiteralTypingReport,
};
use crate::reasoning::{
    Evidence, Inference, Limit, Rdf12LiteralValidator, ReasoningError, ReasoningStore,
    plan::LiteralClause,
};
use rdf_reasoning::Rdf12LiteralRuleId;

/// Execute shared default-map GrdfD1 clauses over inspected semantic occurrences.
///
/// This adds generalized literal typing; it does not certify complete source
/// consistency, enumerate occurrences or implement the entire RDFS procedure.
pub struct Rdf12LiteralTyper {
    limits: LiteralTypingLimits,
    role: LiteralRole,
}
impl Rdf12LiteralTyper {
    /// Select explicit aggregate budgets and one source/target-context phase.
    pub const fn new(limits: LiteralTypingLimits, role: LiteralRole) -> Self {
        Self { limits, role }
    }

    /// Inspect selected literals and stage conclusions from shared GrdfD1 patterns.
    ///
    /// The cursor owns immutable occurrence selection and readers. Reader metadata
    /// and UTF-8 payloads are checked before creating virtual metadata premises.
    /// Unknown encountered datatypes remain opaque even when graph assertions
    /// declare them datatypes. RDF scans cannot satisfy this auxiliary relation.
    ///
    /// Source and target context must use separate attempts/phases; callers must
    /// never infer source inconsistency from invalid target-context metadata.
    ///
    /// # Errors
    /// On any storage/inspection failure, role mismatch or budget exhaustion,
    /// abort the whole attempt, including earlier staged conclusions. A lexical
    /// diagnostic is retained for later role-aware consistency/entailment policy;
    /// no error is silently converted into an entailment answer.
    pub async fn run<S, C>(
        &self,
        store: &mut S,
        cursor: &mut C,
    ) -> Result<LiteralTypingReport, LiteralTypingError<S::Term, S::Error>>
    where
        S: ReasoningStore,
        C: LiteralOccurrenceCursor<Term = S::Term, Error = S::Error>,
    {
        Self::check_cancelled(store)?;
        // bounded: exactly three default-map clauses, independent of graph size.
        let mut clauses = Vec::with_capacity(3);
        for &id in Rdf12LiteralRuleId::ALL {
            clauses.push(
                LiteralClause::compile(id, store)
                    .await
                    .map_err(LiteralTypingError::Reasoning)?,
            );
        }
        let mut report = LiteralTypingReport::default();
        loop {
            Self::check_cancelled(store)?;
            let next = cursor
                .next()
                .await
                .map_err(|error| LiteralTypingError::Reasoning(ReasoningError::Storage(error)))?;
            Self::check_cancelled(store)?;
            let Some((occurrence, mut reader)) = next else {
                return Ok(report);
            };
            if occurrence.role != self.role {
                return Err(LiteralTypingError::RoleMismatch {
                    expected: self.role,
                    occurrence,
                });
            }
            if report.occurrences == self.limits.occurrences.get() {
                return Err(LiteralTypingError::OccurrenceLimit);
            }
            report.occurrences += 1;
            crate::cooperative::checkpoint(report.occurrences).await;
            Self::check_cancelled(store)?;
            let inspection = Rdf12LiteralValidator::new(self.limits.bytes - report.bytes)
                .validate(&mut reader)
                .await
                .map_err(|error| LiteralTypingError::Literal {
                    occurrence: occurrence.clone(),
                    error,
                })?;
            Self::check_cancelled(store)?;
            report.bytes += inspection.bytes;
            let Some(datatype) = inspection.datatype else {
                report.opaque += 1;
                continue;
            };
            let id = Rdf12LiteralRuleId::GrdfD1(datatype);
            let clause = clauses
                .iter()
                .find(|clause| clause.id == id)
                .ok_or_else(|| {
                    LiteralTypingError::Reasoning(ReasoningError::UnsupportedLiteralRule(id))
                })?;
            let metadata = [occurrence.literal.clone(), clause.datatype.clone()];
            let bindings = clause
                .body
                .bind(&metadata, std::array::from_fn(|_| None))
                .ok_or_else(|| {
                    LiteralTypingError::Reasoning(ReasoningError::UnsupportedLiteralRule(id))
                })?;
            let triple = clause.head.ground(&bindings).ok_or_else(|| {
                LiteralTypingError::Reasoning(ReasoningError::UnsupportedLiteralRule(id))
            })?;
            if store
                .contains(&triple)
                .await
                .map_err(|error| LiteralTypingError::Reasoning(ReasoningError::Storage(error)))?
            {
                continue;
            }
            Self::check_cancelled(store)?;
            if report.inferred == self.limits.inferred.get() {
                return Err(LiteralTypingError::Reasoning(
                    ReasoningError::LimitExceeded(Limit::Inferred),
                ));
            }
            let inference = Inference {
                triple,
                evidence: Evidence::Literal {
                    rule: id,
                    occurrence,
                    lexical_bytes: inspection.bytes,
                },
            };
            if store
                .insert_inferred(inference)
                .await
                .map_err(|error| LiteralTypingError::Reasoning(ReasoningError::Storage(error)))?
            {
                report.inferred += 1;
            }
        }
    }
    fn check_cancelled<S: ReasoningStore>(
        store: &S,
    ) -> Result<(), LiteralTypingError<S::Term, S::Error>> {
        if store.is_cancelled() {
            Err(LiteralTypingError::Reasoning(ReasoningError::Cancelled))
        } else {
            Ok(())
        }
    }
}
