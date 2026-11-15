use super::{
    ExecutionProfile, Inference, Limit, Premises, ReasoningError, ReasoningStore, RunLimits,
    RunReport, TripleCursor, TripleSource,
    plan::{Bindings, Body, Clause},
};

/// One native, storage-independent executor for the named RDF/RDFS profiles.
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
        "rdf-reasoning/2.7.0"
    }

    /// Derive into caller-owned scratch until a fixpoint, retaining bounded state.
    ///
    /// Memory is at most fourteen clauses, fixed-size bindings and two active cursors
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
        // bounded: the executable shared selections contain at most fourteen clauses.
        let mut clauses = Vec::with_capacity(self.profile.rules().len());
        for &rule in self.profile.rules() {
            clauses.push(Clause::compile(rule, store).await?);
        }
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
            .scan(clause.body.first().probe(&empty))
            .await
            .map_err(ReasoningError::Storage)?;
        while let Some(first) = self.next(&mut outer, store, report).await? {
            let Some(bindings) = clause.body.first().bind(&first, empty.clone()) else {
                continue;
            };
            let Body::Two([_, second_pattern]) = &clause.body else {
                self.emit(clause, &bindings, Premises::One(first), store, report)
                    .await?;
                continue;
            };
            let mut inner = source
                .scan(second_pattern.probe(&bindings))
                .await
                .map_err(ReasoningError::Storage)?;
            while let Some(second) = self.next(&mut inner, store, report).await? {
                let Some(bindings) = second_pattern.bind(&second, bindings.clone()) else {
                    continue;
                };
                self.emit(
                    clause,
                    &bindings,
                    Premises::Two([first.clone(), second]),
                    store,
                    report,
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn emit<S: ReasoningStore>(
        &self,
        clause: &Clause<S::Term>,
        bindings: &Bindings<S::Term>,
        premises: Premises<S::Term>,
        store: &mut S,
        report: &mut RunReport,
    ) -> Result<(), ReasoningError<S::Error>> {
        let triple = clause.head.ground(bindings, clause.id)?;
        if store
            .contains(&triple)
            .await
            .map_err(ReasoningError::Storage)?
        {
            return Ok(());
        }
        Self::check_cancelled(store)?;
        if report.inferred == self.limits.inferred.get() {
            return Err(ReasoningError::LimitExceeded(Limit::Inferred));
        }
        let inference = Inference {
            triple,
            rule: clause.id,
            premises,
        };
        if store
            .insert_inferred(inference)
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
