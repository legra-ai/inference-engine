//! Bounded forward execution of the shared `rdfs9` and `rdfs11` clauses.
//!
//! Callers own immutable selected input, indexed scratch storage and publication.
//! This module never collects a graph, chooses graphs, or promotes partial output.

mod engine;
mod plan;
mod types;

pub use engine::Reasoner;
pub use types::{
    ExecutionProfile, Inference, Limit, ProfileError, ReasoningError, RunLimits, RunReport,
};

use std::{error::Error, future::Future};

/// A fallible, bounded cursor over an immutable snapshot's matching tuples.
pub trait TripleCursor: Send {
    /// Caller-owned exact RDF term identity; scoped blanks must remain distinct.
    type Term: Clone + Eq + Send + Sync;
    /// Read failures, including cancellation, must never become end of stream.
    type Error: Error + Send + Sync + 'static;
    /// Read at most one tuple. Drop must release/cancel the underlying read.
    fn next(&mut self)
    -> impl Future<Output = Result<Option<[Self::Term; 3]>, Self::Error>> + Send;
}

/// One immutable round view of selected assertions plus staged consequences.
///
/// Scans must be bounded and match every supplied term exactly. The view and
/// its cursors remain valid while the caller inserts into the separate store.
/// No ordering is required; set closure is independent of scan order. The first
/// retained proof can differ between orders and is not a canonical explanation.
pub trait TripleSource: Send + Sync {
    /// Exact term type, without lexical/value normalization by the executor.
    type Term: Clone + Eq + Send + Sync;
    /// Backend read error.
    type Error: Error + Send + Sync + 'static;
    /// Owned cursor, independent of subsequent store writes.
    type Cursor: TripleCursor<Term = Self::Term, Error = Self::Error>;
    /// Open a scan/probe; `None` is a wildcard in subject/predicate/object order.
    fn scan(
        &self,
        pattern: [Option<Self::Term>; 3],
    ) -> impl Future<Output = Result<Self::Cursor, Self::Error>> + Send;
}

/// Backend capabilities for one fresh, isolated inference attempt.
///
/// Initially expose only the caller's selected asserted snapshot, never earlier
/// inferred output. Scratch facts must spill to caller-owned indexed storage;
/// no method may buffer a graph-sized relation. Each snapshot freezes the union
/// as of its creation; subsequent insertions are visible in the next snapshot.
///
/// On any error the caller must discard/abort staged output. A successful run
/// grants no publication authority: callers still check input currentness and
/// atomically promote their owned inference contribution. Recompute from fresh
/// assertions after deletion. The executor never mutates asserted input.
pub trait ReasoningStore: Send {
    /// Exact RDF term identity; database IDs and backend RDF terms both work.
    type Term: Clone + Eq + Send + Sync;
    /// Backend failure propagated without substituting an empty relation.
    type Error: Error + Send + Sync + 'static;
    /// Owned immutable view supporting concurrent outer/inner read cursors.
    type Snapshot: TripleSource<Term = Self::Term, Error = Self::Error>;
    /// Resolve a full rule-constant IRI as an IRI, never as a plain string literal.
    fn resolve_iri(
        &mut self,
        iri: &str,
    ) -> impl Future<Output = Result<Self::Term, Self::Error>> + Send;
    /// Freeze selected assertions plus all currently staged inferred tuples.
    fn snapshot(&mut self) -> impl Future<Output = Result<Self::Snapshot, Self::Error>> + Send;
    /// Indexed membership in the current asserted-plus-staged union.
    fn contains(
        &mut self,
        triple: &[Self::Term; 3],
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Stage a novel inferred tuple with its first proof; assertions win duplicates.
    ///
    /// Return `true` only for a new inferred tuple. Preserve generalized tuples
    /// in scratch; do not silently drop terms or normalize literal lexical forms.
    fn insert_inferred(
        &mut self,
        inference: Inference<Self::Term>,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Whether this attempt is cancelled/superseded. Awaited operations must also
    /// support cancellation through the backend's own error/control boundary.
    fn is_cancelled(&self) -> bool;
}
