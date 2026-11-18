use super::{
    Evidence, Inference, Limit, Rdf12TripleTermStore, ReasoningError, ReasoningStore, TripleCursor,
    TripleSource,
    plan::{Bindings, PropositionClause},
};
use rdf_reasoning::Rdf12PropositionRuleId;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;

/// Work and output budgets for one finite Grdfs14 constructor phase.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropositionLimits {
    inferred: NonZeroU64,
    matches: NonZeroU64,
}
impl PropositionLimits {
    /// Bound novel output tuples and examined rows in the selected constructor mode.
    pub const fn new(inferred: NonZeroU64, matches: NonZeroU64) -> Self {
        Self { inferred, matches }
    }
}

/// Completed counters for one constructor phase, not a recursive fixpoint claim.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropositionReport {
    /// Examined join rows, or candidate plus exact-premise rows in targeted execution.
    pub matches: u64,
    /// Novel proposition-membership tuples, excluding asserted/inferred duplicates.
    pub inferred: u64,
}

/// Execute shared Grdfs14 once over an immutable, explicitly selected snapshot.
///
/// The controller supplies adequate typing and source/target/substitution context
/// before construction. This operation does not certify a complete schedule for
/// the draft standard's postulated Appendix A entailment procedure. Newly created
/// terms must never feed an unrestricted recursive construction loop.
pub struct Rdf12PropositionConstructor {
    limits: PropositionLimits,
}
impl Rdf12PropositionConstructor {
    /// Set strict work/output caps; the backend separately caps dictionary/nesting.
    pub const fn new(limits: PropositionLimits) -> Self {
        Self { limits }
    }

    /// Construct generalized triple terms from three shared typing premises.
    ///
    /// Exactly one snapshot and at most three owned cursors are retained. There
    /// is no graph collection or recursive constructor feedback. Preserve this
    /// phase's snapshot, scope and origins alongside the retained rule/premises.
    /// Contents of constructed terms are never asserted into the graph.
    ///
    /// # Errors
    /// Compilation, storage, substituted-term construction, cancellation and
    /// budget errors abort the entire attempt; discard all partial staged output.
    pub async fn run<S: Rdf12TripleTermStore>(
        &self,
        store: &mut S,
    ) -> Result<PropositionReport, ReasoningError<S::Error>> {
        Self::cancel(store)?;
        let clause = PropositionClause::compile(Rdf12PropositionRuleId::Grdfs14, store).await?;
        let source = store.snapshot().await.map_err(ReasoningError::Storage)?;
        let mut report = PropositionReport::default();
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
                let mut third = source
                    .scan(clause.body[2].probe(&bindings))
                    .await
                    .map_err(ReasoningError::Storage)?;
                while let Some(c) = self.next(&mut third, store, &mut report).await? {
                    let Some(bindings) = clause.body[2].bind(&c, bindings.clone()) else {
                        continue;
                    };
                    self.emit(
                        &clause,
                        &bindings,
                        [a.clone(), b.clone(), c],
                        store,
                        &mut report,
                    )
                    .await?;
                }
            }
        }
        Self::cancel(store)?;
        Ok(report)
    }
    /// Construct only caller-selected subject/predicate/object candidates.
    ///
    /// Candidates are structural tuples, not asserted RDF premises. The controller
    /// streams existing terms or adequately substituted target goals; it remains
    /// responsible for selection completeness and source/target phase ordering.
    /// One immutable snapshot verifies all three exact shared typing premises.
    /// No resource/property Cartesian product or recursive feedback is generated.
    /// Missing premises skip a candidate, not establish non-entailment.
    ///
    /// `matches` counts candidate rows plus examined premise rows. Both empty and
    /// successful premise probes are exhausted, so late backend errors invalidate
    /// prior staging. All ordinary constructor output/depth/cancellation errors
    /// retain their abort semantics. The candidate cursor must be bounded/streamed.
    ///
    /// # Errors
    /// Read/compile/storage, malformed probes, cancellation and budgets fail the
    /// entire attempt; discard all partial staged output and dictionary additions.
    pub async fn run_candidates<S, C>(
        &self,
        store: &mut S,
        candidates: &mut C,
    ) -> Result<PropositionReport, ReasoningError<S::Error>>
    where
        S: Rdf12TripleTermStore,
        C: TripleCursor<Term = S::Term, Error = S::Error>,
    {
        let mut phase = self.begin(store).await?;
        while let Some(candidate) = self.next(candidates, store, &mut phase.report).await? {
            phase.candidate(store, candidate).await?;
        }
        Self::cancel(store)?;
        Ok(phase.report)
    }
    pub(super) async fn begin<S: Rdf12TripleTermStore>(
        &self,
        store: &mut S,
    ) -> Result<PropositionPhase<S::Snapshot>, ReasoningError<S::Error>> {
        Self::cancel(store)?;
        let clause = PropositionClause::compile(Rdf12PropositionRuleId::Grdfs14, store).await?;
        let source = store.snapshot().await.map_err(ReasoningError::Storage)?;
        Ok(PropositionPhase {
            constructor: Self::new(self.limits),
            clause,
            source,
            report: PropositionReport::default(),
        })
    }
    async fn emit<S: Rdf12TripleTermStore>(
        &self,
        clause: &PropositionClause<S::Term>,
        bindings: &Bindings<S::Term>,
        premises: [[S::Term; 3]; 3],
        store: &mut S,
        report: &mut PropositionReport,
    ) -> Result<(), ReasoningError<S::Error>> {
        Self::cancel(store)?;
        let quoted = clause
            .triple
            .ground(bindings)
            .ok_or(ReasoningError::UnsupportedPropositionRule(clause.id))?;
        let term = store
            .resolve_triple_term(quoted)
            .await
            .map_err(ReasoningError::Storage)?;
        let triple = [term, clause.predicate.clone(), clause.object.clone()];
        if store
            .contains(&triple)
            .await
            .map_err(ReasoningError::Storage)?
        {
            return Ok(());
        }
        Self::cancel(store)?;
        if report.inferred == self.limits.inferred.get() {
            return Err(ReasoningError::LimitExceeded(Limit::Inferred));
        }
        if store
            .insert_inferred(Inference {
                triple,
                evidence: Evidence::Proposition {
                    rule: clause.id,
                    premises,
                },
            })
            .await
            .map_err(ReasoningError::Storage)?
        {
            report.inferred += 1;
        }
        Ok(())
    }
    async fn next<S: ReasoningStore, C: TripleCursor<Term = S::Term, Error = S::Error>>(
        &self,
        cursor: &mut C,
        store: &mut S,
        report: &mut PropositionReport,
    ) -> Result<Option<[S::Term; 3]>, ReasoningError<S::Error>> {
        Self::cancel(store)?;
        let row = cursor.next().await.map_err(ReasoningError::Storage)?;
        if row.is_some() {
            self.charge(store, report).await?;
        }
        Ok(row)
    }
    async fn charge<S: ReasoningStore>(
        &self,
        store: &mut S,
        report: &mut PropositionReport,
    ) -> Result<(), ReasoningError<S::Error>> {
        Self::cancel(store)?;
        if report.matches == self.limits.matches.get() {
            return Err(ReasoningError::LimitExceeded(Limit::Matches));
        }
        report.matches += 1;
        crate::cooperative::checkpoint(report.matches).await;
        Self::cancel(store)
    }
    fn cancel<S: ReasoningStore>(store: &S) -> Result<(), ReasoningError<S::Error>> {
        if store.is_cancelled() {
            Err(ReasoningError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// A single fixed-premise-snapshot phase shared by manual and automatic selection.
pub(super) struct PropositionPhase<P: TripleSource> {
    constructor: Rdf12PropositionConstructor,
    clause: PropositionClause<P::Term>,
    source: P,
    pub(super) report: PropositionReport,
}
impl<P: TripleSource> PropositionPhase<P> {
    pub(super) fn source(&self) -> &P {
        &self.source
    }
    pub(super) async fn candidate<S>(
        &mut self,
        store: &mut S,
        candidate: [P::Term; 3],
    ) -> Result<(), ReasoningError<P::Error>>
    where
        S: Rdf12TripleTermStore<Term = P::Term, Error = P::Error>,
    {
        Rdf12PropositionConstructor::cancel(store)?;
        let clause = &self.clause;
        let bindings = clause
            .triple
            .bind(&candidate, std::array::from_fn(|_| None))
            .ok_or(ReasoningError::UnsupportedPropositionRule(clause.id))?;
        // bounded: exactly three shared typing premises, independent of graph size.
        let mut premises = std::array::from_fn(|_| candidate.clone());
        for (index, pattern) in clause.body.iter().enumerate() {
            let premise = pattern
                .ground(&bindings)
                .ok_or(ReasoningError::UnsupportedPropositionRule(clause.id))?;
            let mut probe = self
                .source
                .scan(premise.clone().map(Some))
                .await
                .map_err(ReasoningError::Storage)?;
            let mut found = false;
            while let Some(row) = self
                .constructor
                .next(&mut probe, store, &mut self.report)
                .await?
            {
                if row != premise {
                    return Err(ReasoningError::InvalidProbe);
                }
                found = true;
            }
            if !found {
                return Ok(());
            }
            premises[index] = premise;
        }
        self.constructor
            .emit(clause, &bindings, premises, store, &mut self.report)
            .await
    }
    pub(super) async fn selected<S>(
        &mut self,
        store: &mut S,
        candidate: [P::Term; 3],
    ) -> Result<(), ReasoningError<P::Error>>
    where
        S: Rdf12TripleTermStore<Term = P::Term, Error = P::Error>,
    {
        self.constructor.charge(store, &mut self.report).await?;
        self.candidate(store, candidate).await
    }
}
