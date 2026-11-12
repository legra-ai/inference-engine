use super::{
    ExecutionProfile, Inference, Limit, ReasoningError, ReasoningStore, RunLimits, RunReport,
    TripleCursor, TripleSource, plan::Clause,
};

/// One native, storage-independent executor for the named two-rule profile.
pub struct Reasoner {
    profile: ExecutionProfile,
    limits: RunLimits,
}

impl Reasoner {
    /// Select an exact supported profile and explicit resource limits.
    pub const fn new(profile: ExecutionProfile, limits: RunLimits) -> Self {
        Self { profile, limits }
    }
    /// Exact selected execution contract.
    pub const fn profile(&self) -> ExecutionProfile {
        self.profile
    }
    /// Implementation package version to record alongside input/profile identity.
    pub const fn implementation_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
    /// Pinned semantic rule catalogue identity for inference provenance.
    pub const fn rule_catalogue(&self) -> &'static str {
        "rdf-reasoning/2.6.0"
    }

    /// Derive into caller-owned scratch until a fixpoint, retaining bounded state.
    ///
    /// Memory is two compiled clauses, fixed-size bindings and two active cursors
    /// per join. Each round reads an immutable view; inserted tuples become
    /// join inputs next round. Stores must implement indexed probes and disk spill.
    ///
    /// # Errors
    /// Cancellation, resource exhaustion, unsupported shared rule shapes or any
    /// storage failure abort the run. Staged output must then be discarded.
    pub async fn run<S: ReasoningStore>(
        &self,
        store: &mut S,
    ) -> Result<RunReport, ReasoningError<S::Error>> {
        Self::check_cancelled(store)?;
        let [first, second] = *self.profile.rules();
        let clauses = [
            Clause::compile(first, store).await?,
            Clause::compile(second, store).await?,
        ];
        let mut report = RunReport::default();
        loop {
            Self::check_cancelled(store)?;
            if report.rounds == self.limits.rounds.get() {
                return Err(ReasoningError::LimitExceeded(Limit::Rounds));
            }
            report.rounds += 1;
            let snapshot = store.snapshot().await.map_err(ReasoningError::Storage)?;
            let before = report.inferred;
            for clause in &clauses {
                self.evaluate(clause, &snapshot, store, &mut report).await?;
            }
            Self::check_cancelled(store)?;
            if report.inferred == before {
                return Ok(report);
            }
        }
    }

    async fn evaluate<S: ReasoningStore>(
        &self,
        clause: &Clause<S::Term>,
        source: &S::Snapshot,
        store: &mut S,
        report: &mut RunReport,
    ) -> Result<(), ReasoningError<S::Error>> {
        let empty = std::array::from_fn(|_| None);
        let mut outer = source
            .scan(clause.body[0].probe(&empty))
            .await
            .map_err(ReasoningError::Storage)?;
        while let Some(first) = self.next(&mut outer, store, report).await? {
            let Some(bindings) = clause.body[0].bind(&first, empty.clone()) else {
                continue;
            };
            let mut inner = source
                .scan(clause.body[1].probe(&bindings))
                .await
                .map_err(ReasoningError::Storage)?;
            while let Some(second) = self.next(&mut inner, store, report).await? {
                let Some(bindings) = clause.body[1].bind(&second, bindings.clone()) else {
                    continue;
                };
                let triple = clause.head.ground(&bindings, clause.id)?;
                if store
                    .contains(&triple)
                    .await
                    .map_err(ReasoningError::Storage)?
                {
                    continue;
                }
                Self::check_cancelled(store)?;
                if report.inferred == self.limits.inferred.get() {
                    return Err(ReasoningError::LimitExceeded(Limit::Inferred));
                }
                let inference = Inference {
                    triple,
                    rule: clause.id,
                    premises: [first.clone(), second],
                };
                if store
                    .insert_inferred(inference)
                    .await
                    .map_err(ReasoningError::Storage)?
                {
                    report.inferred += 1;
                }
            }
        }
        Ok(())
    }

    async fn next<S: ReasoningStore, C: TripleCursor<Term = S::Term, Error = S::Error>>(
        &self,
        cursor: &mut C,
        store: &mut S,
        report: &mut RunReport,
    ) -> Result<Option<[S::Term; 3]>, ReasoningError<S::Error>> {
        Self::check_cancelled(store)?;
        let tuple = cursor.next().await.map_err(ReasoningError::Storage)?;
        if tuple.is_some() {
            if report.matches == self.limits.matches.get() {
                return Err(ReasoningError::LimitExceeded(Limit::Matches));
            }
            report.matches += 1;
        }
        Ok(tuple)
    }

    fn check_cancelled<S: ReasoningStore>(store: &S) -> Result<(), ReasoningError<S::Error>> {
        if store.is_cancelled() {
            Err(ReasoningError::Cancelled)
        } else {
            Ok(())
        }
    }
}
