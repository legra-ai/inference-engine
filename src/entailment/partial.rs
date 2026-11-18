use super::{EntailmentError, EntailmentLimits, EntailmentTerms, matcher::State};
use crate::reasoning::{TripleCursor, TripleSource};

struct Frame<C> {
    cursor: Option<C>,
    checkpoint: usize,
    skipped: bool,
}

/// Enumerate consistent bindings from every matched subset of a capped target.
/// Skipping is a search choice, never proof of that pattern or of the whole target.
pub(crate) struct PartialMatches<C: TripleCursor> {
    state: State,
    // bounded: limits.patterns caps target patterns and simultaneously open cursors.
    patterns: Vec<[C::Term; 3]>,
    frames: Vec<Frame<C>>,
    // bounded: limits.bindings caps active, globally consistent assignments.
    bindings: Vec<(C::Term, C::Term)>,
    empty_returned: bool,
}
impl<C: TripleCursor> PartialMatches<C> {
    pub(crate) async fn new<S, T, A>(
        source: &S,
        target: &T,
        terms: &A,
        limits: EntailmentLimits,
    ) -> Result<Self, EntailmentError<C::Error>>
    where
        S: TripleSource<Term = C::Term, Error = C::Error, Cursor = C>,
        T: TripleSource<Term = C::Term, Error = C::Error>,
        A: EntailmentTerms<Term = C::Term, Error = C::Error>,
    {
        let mut state = State { limits, steps: 0 };
        state.step(terms).await?;
        let mut cursor = target
            .scan([None, None, None])
            .await
            .map_err(EntailmentError::Storage)?;
        // bounded: reject before retaining more than the explicit target-pattern cap.
        let mut patterns = Vec::new();
        loop {
            state.step(terms).await?;
            let row = cursor.next().await.map_err(EntailmentError::Storage)?;
            state.step(terms).await?;
            let Some(row) = row else { break };
            if patterns.len() == limits.patterns.get() as usize {
                return Err(EntailmentError::TargetLimit);
            }
            patterns.push(row);
        }
        let mut result = Self {
            state,
            patterns,
            frames: Vec::new(),
            bindings: Vec::new(),
            empty_returned: false,
        };
        if !result.patterns.is_empty() {
            result.descend(source, terms).await?;
        }
        Ok(result)
    }
    pub(crate) fn patterns(&self) -> &[[C::Term; 3]] {
        &self.patterns
    }
    pub(crate) fn bindings(&self) -> &[(C::Term, C::Term)] {
        &self.bindings
    }
    pub(crate) fn steps(&self) -> u64 {
        self.state.steps
    }

    pub(crate) async fn next<S, A>(
        &mut self,
        source: &S,
        terms: &A,
    ) -> Result<bool, EntailmentError<C::Error>>
    where
        S: TripleSource<Term = C::Term, Error = C::Error, Cursor = C>,
        A: EntailmentTerms<Term = C::Term, Error = C::Error>,
    {
        self.state.step(terms).await?;
        if self.patterns.is_empty() {
            let first = !self.empty_returned;
            self.empty_returned = true;
            return Ok(first);
        }
        while let Some(frame) = self.frames.last_mut() {
            self.bindings.truncate(frame.checkpoint);
            self.state.step(terms).await?;
            let row = match &mut frame.cursor {
                Some(cursor) => cursor.next().await.map_err(EntailmentError::Storage)?,
                None => None,
            };
            self.state.step(terms).await?;
            let index = self.frames.len() - 1;
            if let Some(row) = row {
                if !self
                    .state
                    .bind(&self.patterns[index], &row, &mut self.bindings, terms)
                    .await?
                {
                    continue;
                }
            } else {
                let frame = &mut self.frames[index];
                frame.cursor = None;
                if frame.skipped {
                    self.frames.pop();
                    continue;
                }
                frame.skipped = true;
            }
            if index + 1 == self.patterns.len() {
                return Ok(true);
            }
            self.descend(source, terms).await?;
        }
        Ok(false)
    }
    async fn descend<S, A>(
        &mut self,
        source: &S,
        terms: &A,
    ) -> Result<(), EntailmentError<C::Error>>
    where
        S: TripleSource<Term = C::Term, Error = C::Error, Cursor = C>,
        A: EntailmentTerms<Term = C::Term, Error = C::Error>,
    {
        let probe = self
            .state
            .probe(&self.patterns[self.frames.len()], &self.bindings, terms)
            .await?;
        self.state.step(terms).await?;
        let cursor = source.scan(probe).await.map_err(EntailmentError::Storage)?;
        self.frames.push(Frame {
            cursor: Some(cursor),
            checkpoint: self.bindings.len(),
            skipped: false,
        });
        Ok(())
    }
}
