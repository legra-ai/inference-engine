use super::{EntailmentError, EntailmentLimits, EntailmentReport, EntailmentTerms};
use crate::reasoning::{TripleCursor, TripleSource};

/// Source-to-target simple entailment by consistent existential graph matching.
pub struct SimpleEntailment {
    limits: EntailmentLimits,
}

struct Frame<C> {
    cursor: C,
    checkpoint: usize,
}
struct State {
    limits: EntailmentLimits,
    steps: u64,
}

impl SimpleEntailment {
    /// Select finite target, binding, recursive-worklist and work budgets.
    pub const fn new(limits: EntailmentLimits) -> Self {
        Self { limits }
    }

    /// Decide whether the source graph contains one instance of the target.
    ///
    /// Source/target snapshots and term identity must be immutable for the whole
    /// call. Source triples are streamed through indexed scans. Only explicitly
    /// capped target patterns, bindings, owned cursors and term pairs are retained.
    /// Triple terms are decomposed for substitution and never added as assertions.
    ///
    /// # Errors
    /// Any read failure, cancellation or exhausted cap aborts the decision.
    /// Resource exhaustion must never be interpreted as non-entailment.
    pub async fn check<S, Q, A>(
        &self,
        source: &S,
        target: &Q,
        terms: &A,
    ) -> Result<EntailmentReport, EntailmentError<S::Error>>
    where
        S: TripleSource,
        Q: TripleSource<Term = S::Term, Error = S::Error>,
        A: EntailmentTerms<Term = S::Term, Error = S::Error>,
    {
        let mut state = State {
            limits: self.limits,
            steps: 0,
        };
        state.step(terms).await?;
        let mut cursor = target
            .scan([None, None, None])
            .await
            .map_err(EntailmentError::Storage)?;
        // bounded: reject before retaining more than limits.patterns target tuples.
        let mut patterns = Vec::new();
        loop {
            state.step(terms).await?;
            let tuple = cursor.next().await.map_err(EntailmentError::Storage)?;
            state.cancel(terms)?;
            let Some(tuple) = tuple else {
                break;
            };
            if patterns.len() == self.limits.patterns.get() as usize {
                return Err(EntailmentError::TargetLimit);
            }
            patterns.push(tuple);
        }
        drop(cursor);
        if patterns.is_empty() {
            return Ok(state.report(true, 0));
        }
        // bounded: limits.bindings caps retained existential assignments before insertion.
        let mut bindings = Vec::new();
        // bounded: one owned cursor per capped target pattern; no source relation is collected.
        let mut frames = Vec::new();
        let probe = state.probe(&patterns[0], &bindings, terms).await?;
        state.step(terms).await?;
        frames.push(Frame {
            cursor: source.scan(probe).await.map_err(EntailmentError::Storage)?,
            checkpoint: 0,
        });
        while let Some(frame) = frames.last_mut() {
            bindings.truncate(frame.checkpoint);
            state.step(terms).await?;
            let tuple = frame
                .cursor
                .next()
                .await
                .map_err(EntailmentError::Storage)?;
            state.cancel(terms)?;
            let Some(tuple) = tuple else {
                frames.pop();
                continue;
            };
            let index = frames.len() - 1;
            if !state
                .bind(&patterns[index], &tuple, &mut bindings, terms)
                .await?
            {
                continue;
            }
            if index + 1 == patterns.len() {
                return Ok(state.report(true, patterns.len() as u32));
            }
            let probe = state.probe(&patterns[index + 1], &bindings, terms).await?;
            state.step(terms).await?;
            frames.push(Frame {
                cursor: source.scan(probe).await.map_err(EntailmentError::Storage)?,
                checkpoint: bindings.len(),
            });
        }
        state.cancel(terms)?;
        Ok(state.report(false, patterns.len() as u32))
    }
}

impl State {
    fn report(&self, entailed: bool, target_triples: u32) -> EntailmentReport {
        EntailmentReport {
            entailed,
            target_triples,
            steps: self.steps,
        }
    }
    fn cancel<A: EntailmentTerms>(&self, terms: &A) -> Result<(), EntailmentError<A::Error>> {
        if terms.is_cancelled() {
            Err(EntailmentError::Cancelled)
        } else {
            Ok(())
        }
    }
    async fn step<A: EntailmentTerms>(
        &mut self,
        terms: &A,
    ) -> Result<(), EntailmentError<A::Error>> {
        self.cancel(terms)?;
        if self.steps == self.limits.steps.get() {
            return Err(EntailmentError::StepLimit);
        }
        self.steps += 1;
        crate::cooperative::checkpoint(self.steps).await;
        self.cancel(terms)
    }
    async fn probe<A: EntailmentTerms>(
        &mut self,
        pattern: &[A::Term; 3],
        bindings: &[(A::Term, A::Term)],
        terms: &A,
    ) -> Result<[Option<A::Term>; 3], EntailmentError<A::Error>> {
        let mut probe = [None, None, None];
        for (index, term) in pattern.iter().enumerate() {
            self.step(terms).await?;
            if terms
                .is_blank(term)
                .await
                .map_err(EntailmentError::Storage)?
            {
                probe[index] = bindings
                    .iter()
                    .find(|(key, _)| key == term)
                    .map(|(_, value)| value.clone());
            } else if terms
                .triple_term(term)
                .await
                .map_err(EntailmentError::Storage)?
                .is_none()
            {
                probe[index] = Some(term.clone());
            }
            self.cancel(terms)?;
        }
        Ok(probe)
    }
    async fn bind<A: EntailmentTerms>(
        &mut self,
        pattern: &[A::Term; 3],
        tuple: &[A::Term; 3],
        bindings: &mut Vec<(A::Term, A::Term)>,
        terms: &A,
    ) -> Result<bool, EntailmentError<A::Error>> {
        // bounded: limits.terms caps pending pairs, including the initial three slots.
        let mut pending = Vec::new();
        self.push_pairs(&mut pending, pattern, tuple)?;
        while let Some((target, candidate)) = pending.pop() {
            self.step(terms).await?;
            if terms
                .is_blank(&target)
                .await
                .map_err(EntailmentError::Storage)?
            {
                if let Some((_, value)) = bindings.iter().find(|(key, _)| key == &target) {
                    if value != &candidate {
                        return Ok(false);
                    }
                } else {
                    if bindings.len() == self.limits.bindings.get() as usize {
                        return Err(EntailmentError::BindingLimit);
                    }
                    bindings.push((target, candidate));
                }
            } else if let Some(target) = terms
                .triple_term(&target)
                .await
                .map_err(EntailmentError::Storage)?
            {
                let Some(candidate) = terms
                    .triple_term(&candidate)
                    .await
                    .map_err(EntailmentError::Storage)?
                else {
                    return Ok(false);
                };
                self.push_pairs(&mut pending, &target, &candidate)?;
            } else if target != candidate {
                return Ok(false);
            }
            self.cancel(terms)?;
        }
        Ok(true)
    }
    fn push_pairs<T: Clone, E>(
        &self,
        pending: &mut Vec<(T, T)>,
        target: &[T; 3],
        candidate: &[T; 3],
    ) -> Result<(), EntailmentError<E>> {
        if pending.len().saturating_add(3) > self.limits.terms.get() as usize {
            return Err(EntailmentError::TermLimit);
        }
        for (target, candidate) in target.iter().zip(candidate).rev() {
            pending.push((target.clone(), candidate.clone()));
        }
        Ok(())
    }
}
