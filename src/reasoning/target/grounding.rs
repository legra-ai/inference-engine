use super::{TargetConstructionError, TargetConstructionLimits};
use crate::{
    entailment::{EntailmentError, EntailmentTerms},
    reasoning::{
        Rdf12TripleTermStore, ReasoningError, ReasoningStore, TripleSource,
        proposition::PropositionPhase,
    },
};
enum Task<T> {
    Visit(T),
    Finish,
}
pub(super) struct Grounding {
    limits: TargetConstructionLimits,
    steps: u64,
}
impl Grounding {
    pub(super) const fn new(limits: TargetConstructionLimits) -> Self {
        Self { limits, steps: 0 }
    }
    pub(super) const fn steps(&self) -> u64 {
        self.steps
    }
    pub(super) async fn has_goals<S, A>(
        &mut self,
        store: &mut S,
        access: &A,
        patterns: &[[S::Term; 3]],
    ) -> Result<bool, TargetConstructionError<S::Error>>
    where
        S: ReasoningStore,
        A: EntailmentTerms<Term = S::Term, Error = S::Error>,
    {
        let mut found = false;
        for pattern in patterns {
            for term in pattern {
                self.finish(store, access).await?;
                found |= access
                    .triple_term(term)
                    .await
                    .map_err(ReasoningError::Storage)?
                    .is_some();
            }
        }
        Ok(found)
    }
    pub(super) async fn run<S, A, P>(
        &mut self,
        store: &mut S,
        access: &A,
        term: &S::Term,
        bindings: &[(S::Term, S::Term)],
        representative: &S::Term,
        phase: &mut PropositionPhase<P>,
    ) -> Result<(), TargetConstructionError<S::Error>>
    where
        S: Rdf12TripleTermStore,
        A: EntailmentTerms<Term = S::Term, Error = S::Error>,
        P: TripleSource<Term = S::Term, Error = S::Error>,
    {
        // bounded: combined pending tasks and grounded values are capped before every push.
        let mut tasks = vec![Task::Visit(term.clone())];
        let mut values = Vec::new();
        while let Some(task) = tasks.pop() {
            self.finish(store, access).await?;
            match task {
                Task::Visit(term) => {
                    if access
                        .is_blank(&term)
                        .await
                        .map_err(ReasoningError::Storage)?
                    {
                        let value = bindings
                            .iter()
                            .find(|(key, _)| key == &term)
                            .map_or(representative, |(_, value)| value)
                            .clone();
                        self.slots(tasks.len(), values.len(), 1)?;
                        values.push(value);
                    } else if let Some(parts) = access
                        .triple_term(&term)
                        .await
                        .map_err(ReasoningError::Storage)?
                    {
                        self.slots(tasks.len(), values.len(), 4)?;
                        tasks.push(Task::Finish);
                        for part in parts.into_iter().rev() {
                            tasks.push(Task::Visit(part));
                        }
                    } else {
                        self.slots(tasks.len(), values.len(), 1)?;
                        values.push(term);
                    }
                }
                Task::Finish => {
                    let object = values.pop().expect("three grounded triple components");
                    let predicate = values.pop().expect("grounded predicate");
                    let subject = values.pop().expect("grounded subject");
                    let candidate = [subject, predicate, object];
                    let key = store
                        .resolve_triple_term(candidate.clone())
                        .await
                        .map_err(ReasoningError::Storage)?;
                    if access
                        .triple_term(&key)
                        .await
                        .map_err(ReasoningError::Storage)?
                        != Some(candidate.clone())
                    {
                        return Err(TargetConstructionError::Identity);
                    }
                    phase.selected(store, candidate).await?;
                    self.slots(tasks.len(), values.len(), 1)?;
                    values.push(key);
                }
            }
        }
        Ok(())
    }
    fn slots<E>(
        &self,
        tasks: usize,
        values: usize,
        additional: usize,
    ) -> Result<(), TargetConstructionError<E>> {
        if tasks.saturating_add(values).saturating_add(additional)
            > self.limits.slots.get() as usize
        {
            Err(TargetConstructionError::SlotLimit)
        } else {
            Ok(())
        }
    }
    pub(super) async fn finish<S, A>(
        &mut self,
        store: &mut S,
        access: &A,
    ) -> Result<(), TargetConstructionError<S::Error>>
    where
        S: ReasoningStore,
        A: EntailmentTerms<Term = S::Term, Error = S::Error>,
    {
        if store.is_cancelled() || access.is_cancelled() {
            return Err(EntailmentError::Cancelled.into());
        }
        if self.steps == self.limits.work.get() {
            return Err(TargetConstructionError::WorkLimit);
        }
        self.steps += 1;
        crate::cooperative::checkpoint(self.steps).await;
        if store.is_cancelled() || access.is_cancelled() {
            return Err(EntailmentError::Cancelled.into());
        }
        Ok(())
    }
}
