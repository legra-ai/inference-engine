//! Bounded forward execution of shared positive RDF/RDFS structural clauses.
//!
//! Callers own immutable selected input, indexed scratch storage and publication.
//! This module never collects a graph, chooses graphs, or promotes partial output.

mod conflict;
mod context;
mod engine;
mod literal;
mod literal_typing;
mod occurrence;
mod occurrence_typing;
mod plan;
mod proposition;
mod seeding;
mod source;
mod target;
mod types;

pub use conflict::{
    DatatypeConflict, DatatypeConflictError, DatatypeConflictLimits, DatatypeConflictReport,
    DatatypeConflictSink, Rdf12DatatypeConflictChecker,
};
pub use context::{
    Rdfs12Context, Rdfs12ContextConfig, Rdfs12ContextError, Rdfs12ContextOutcome,
    Rdfs12TargetConfig, Rdfs12TargetReport,
};
pub use engine::Reasoner;
pub use literal::{
    LexicalViolation, LiteralError, LiteralInspection, LiteralReader, LiteralReport,
    Rdf12LiteralValidator,
};
pub use literal_typing::{
    LiteralCursorError, LiteralOccurrence, LiteralOccurrenceCursor, LiteralOccurrenceRead,
    LiteralRole, LiteralTermAccess, LiteralTypingError, LiteralTypingLimits, LiteralTypingReport,
    Rdf12LiteralOccurrences, Rdf12LiteralTyper,
};
pub use occurrence::{
    OccurrenceError, OccurrenceLimits, OccurrencePath, Rdf12OccurrenceCursor, TermOccurrence,
    TermPosition,
};
pub use occurrence_typing::{OccurrenceTypingError, OccurrenceTypingReport, Rdf12OccurrenceTyper};
pub use proposition::{PropositionLimits, PropositionReport, Rdf12PropositionConstructor};
pub use seeding::Rdf12AxiomSeeder;
pub use source::{
    Rdf12ContainerAccess, Rdf12SourceDiagnostic, Rdf12SourceDiagnosticSink, Rdfs12Source,
    Rdfs12SourceConfig, Rdfs12SourceError, Rdfs12SourceOutcome, Rdfs12SourceReport,
    SourceLiteralLimits,
};
pub use target::{
    Rdf12TargetConstructor, TargetConstructionError, TargetConstructionLimits,
    TargetConstructionReport,
};
pub use types::{
    AxiomFamily, Evidence, ExecutionProfile, Inference, Limit, Premises, ProfileError,
    ReasoningError, RunLimits, RunReport,
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
/// inferred output. Explicit axiom seeding may precede rule execution within
/// the same fresh attempt. Scratch facts must spill to caller-owned indexed storage;
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

/// Caller-owned fresh blank allocation for standard nonempty-class requirements.
///
/// Keep bindings in the isolated attempt's storage, keyed by semantic descriptor
/// and immutable scope. Allocation is independent of lexical representatives or
/// user-visible IRI minting; unsupported blank allocation must fail explicitly.
pub trait Rdf12WitnessStore: ReasoningStore {
    /// Resolve one fresh scoped RDF blank witness, reusing the persisted key for
    /// repeated calls with the same descriptor in this attempt. Standardize it
    /// apart from input and target blanks and witnesses in different attempts.
    /// Never return a fixed user IRI, Skolem IRI or chosen literal value.
    fn resolve_existential_blank(
        &mut self,
        id: rdf_reasoning::Rdf12ExistentialAxiomId,
    ) -> impl Future<Output = Result<Self::Term, Self::Error>> + Send;
}

/// Explicit generalized triple-term construction in the caller's dictionary.
pub trait Rdf12TripleTermStore: ReasoningStore {
    /// Resolve a canonical subject/predicate/object triple term without asserting
    /// its contents. Preserve generalized keys, including literal predicates.
    /// Enforce actual substituted nesting and dictionary/resource limits here.
    /// Unsupported terms or exhausted limits must return an error, never a
    /// coerced term, omitted fact or alternate IRI representation.
    fn resolve_triple_term(
        &mut self,
        triple: [Self::Term; 3],
    ) -> impl Future<Output = Result<Self::Term, Self::Error>> + Send;
}
