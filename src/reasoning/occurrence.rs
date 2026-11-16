use super::{LiteralRole, TripleCursor};
use crate::entailment::EntailmentTerms;
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    num::{NonZeroU32, NonZeroU64},
};

/// One position in an asserted or nested subject/predicate/object triple.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TermPosition {
    /// Subject position.
    Subject,
    /// Predicate position, including predicates inside triple terms.
    Predicate,
    /// Object position.
    Object,
}
impl TermPosition {
    fn at(index: u8) -> Self {
        match index {
            0 => Self::Subject,
            1 => Self::Predicate,
            2 => Self::Object,
            _ => unreachable!("triple position"),
        }
    }
}

/// Nonempty structural path from a selected root triple to an appearing term.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccurrencePath {
    root: TermPosition,
    nested: Vec<TermPosition>,
}
impl OccurrencePath {
    /// Iterate the root position followed by zero or more nested positions.
    pub fn positions(&self) -> impl DoubleEndedIterator<Item = &TermPosition> {
        std::iter::once(&self.root).chain(self.nested.iter())
    }
    fn is_predicate(&self) -> bool {
        self.nested.last().unwrap_or(&self.root) == &TermPosition::Predicate
    }
}

/// An actual term appearance, retaining its selected root, path and semantic role.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermOccurrence<T> {
    /// Exact term key, including scoped blanks and triple terms themselves.
    pub term: T,
    /// Original selected root; nested triple contents are not asserted roots.
    pub root: [T; 3],
    /// Source and target appearances must remain separate capabilities.
    pub role: LiteralRole,
    /// Nonempty path through actual triple structure, never literal metadata.
    pub path: OccurrencePath,
}
impl<T> TermOccurrence<T> {
    /// Whether this occurrence occupies a root or nested predicate position.
    pub fn is_predicate(&self) -> bool {
        self.path.is_predicate()
    }
}

/// Explicit bounds on traversal depth, emitted appearances and total read work.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccurrenceLimits {
    depth: NonZeroU32,
    occurrences: NonZeroU64,
    steps: NonZeroU64,
}
impl OccurrenceLimits {
    /// Depth counts path positions, so one permits only non-triple root terms.
    /// Steps count root-cursor reads and authoritative term decompositions.
    pub const fn new(depth: NonZeroU32, occurrences: NonZeroU64, steps: NonZeroU64) -> Self {
        Self {
            depth,
            occurrences,
            steps,
        }
    }
}

/// Incomplete traversal; never interpret errors as absent terms or end of stream.
#[derive(Debug)]
pub enum OccurrenceError<E> {
    /// Root scan or authoritative term access failed.
    Storage(E),
    /// Attempt cancelled or superseded.
    Cancelled,
    /// Another nested frame would exceed the explicit path-depth budget.
    DepthLimit,
    /// Another emitted appearance would exceed its budget.
    OccurrenceLimit,
    /// Another root read or term lookup would exceed its work budget.
    StepLimit,
}
impl<E: fmt::Display> fmt::Display for OccurrenceError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(f, "occurrence access failed: {error}"),
            Self::Cancelled => f.write_str("occurrence traversal cancelled"),
            Self::DepthLimit => f.write_str("occurrence path depth limit exceeded"),
            Self::OccurrenceLimit => f.write_str("occurrence count limit exceeded"),
            Self::StepLimit => f.write_str("occurrence work limit exceeded"),
        }
    }
}
impl<E: Error + 'static> Error for OccurrenceError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            _ => None,
        }
    }
}

struct Frame<T> {
    triple: [T; 3],
    next: u8,
}

/// Stream actual RDF term/predicate appearances with bounded iterative traversal.
///
/// Reuses the asynchronous structural accessor used by simple entailment. A
/// backend cache miss must load authoritative structure or fail; `None` means
/// a supported atomic term, never an evicted or unsupported triple term.
/// Literal datatype/language/direction metadata are not traversed as RDF terms.
///
/// Supply an immutable root cursor for one selected scope and role. Source
/// assertions/derived facts and target context are separately scheduled by the
/// controller. This cursor performs no entailment, datatype typing, property
/// typing, constructor execution or graph writes. After any error discard the
/// traversal and all semantic output based on its incomplete appearances.
pub struct Rdf12OccurrenceCursor<'a, A: EntailmentTerms, C: TripleCursor> {
    roots: C,
    terms: &'a A,
    role: LiteralRole,
    limits: OccurrenceLimits,
    root: Option<[A::Term; 3]>,
    // bounded: one three-key frame per explicitly capped path depth.
    frames: Vec<Frame<A::Term>>,
    occurrences: u64,
    steps: u64,
    finished: bool,
}
impl<'a, A, C> Rdf12OccurrenceCursor<'a, A, C>
where
    A: EntailmentTerms,
    C: TripleCursor<Term = A::Term, Error = A::Error>,
{
    /// Bind one immutable selected root cursor and exact shared term dictionary.
    /// Keep snapshot/source-graph provenance alongside returned root/path records.
    pub fn new(roots: C, terms: &'a A, role: LiteralRole, limits: OccurrenceLimits) -> Self {
        Self {
            roots,
            terms,
            role,
            limits,
            root: None,
            frames: Vec::new(),
            occurrences: 0,
            steps: 0,
            finished: false,
        }
    }
    /// Emit one actual appearance, revisiting repeated keys at distinct paths.
    ///
    /// # Errors
    /// Backend, cancellation and explicit traversal budgets abort the attempt.
    /// Nested cycles cannot exhaust the call stack: depth/work caps fail first.
    pub async fn next(
        &mut self,
    ) -> Result<Option<TermOccurrence<A::Term>>, OccurrenceError<A::Error>> {
        self.cancel()?;
        if self.finished {
            return Ok(None);
        }
        loop {
            if self.frames.is_empty() {
                self.step().await?;
                let Some(root) = self.roots.next().await.map_err(OccurrenceError::Storage)? else {
                    self.cancel()?;
                    self.finished = true;
                    return Ok(None);
                };
                self.root = Some(root.clone());
                self.frames.push(Frame {
                    triple: root,
                    next: 0,
                });
            }
            let frame = self.frames.last_mut().expect("root frame exists");
            if frame.next == 3 {
                self.frames.pop();
                continue;
            }
            let term = frame.triple[usize::from(frame.next)].clone();
            frame.next += 1;
            if self.occurrences == self.limits.occurrences.get() {
                return Err(OccurrenceError::OccurrenceLimit);
            }
            self.step().await?;
            let nested = self
                .terms
                .triple_term(&term)
                .await
                .map_err(OccurrenceError::Storage)?;
            self.cancel()?;
            // bounded: path is at most the configured number of retained frames.
            let path = OccurrencePath {
                root: TermPosition::at(self.frames[0].next - 1),
                nested: self
                    .frames
                    .iter()
                    .skip(1)
                    .map(|frame| TermPosition::at(frame.next - 1))
                    .collect(),
            };
            if let Some(triple) = nested {
                if self.frames.len() == self.limits.depth.get() as usize {
                    return Err(OccurrenceError::DepthLimit);
                }
                self.frames.push(Frame { triple, next: 0 });
            }
            self.occurrences += 1;
            return Ok(Some(TermOccurrence {
                term,
                root: self.root.clone().expect("selected root exists"),
                role: self.role,
                path,
            }));
        }
    }
    async fn step(&mut self) -> Result<(), OccurrenceError<A::Error>> {
        self.cancel()?;
        if self.steps == self.limits.steps.get() {
            return Err(OccurrenceError::StepLimit);
        }
        self.steps += 1;
        crate::cooperative::checkpoint(self.steps).await;
        self.cancel()
    }
    fn cancel(&self) -> Result<(), OccurrenceError<A::Error>> {
        if self.terms.is_cancelled() {
            Err(OccurrenceError::Cancelled)
        } else {
            Ok(())
        }
    }
}
