//! Bounded RDF 1.2 simple-entailment matching over caller-owned immutable graphs.
//!
//! This checks one consistent target blank-node substitution, including inside
//! nested triple terms. RDFS closure, datatype value equivalence, inconsistency
//! detection and target-relevant vocabulary seeding are separate prerequisites
//! for the stronger entailment regimes. A simple match is not full RDFS support.

mod matcher;
pub use matcher::SimpleEntailment;

use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    future::Future,
    num::{NonZeroU32, NonZeroU64},
};

/// Asynchronous structural access to exact source/target term identities.
///
/// Both graphs and this accessor must share a term-identity domain. Preserve
/// scoped blanks and exact RDF literal identity, including case-insensitive
/// language-tag identity and direction. Use bounded-clone IDs or shared handles;
/// no method may hydrate a graph or a dictionary. Reject unsupported extensions
/// at the adapter's standard-RDF admission boundary, never erase or coerce them.
pub trait EntailmentTerms: Send + Sync {
    /// Exact RDF term key, including recursive triple-term identity.
    type Term: Clone + Eq + Send + Sync;
    /// Backend term-access failure.
    type Error: Error + Send + Sync + 'static;
    /// Whether this term is a blank node. Target blanks are existential variables.
    fn is_blank(&self, term: &Self::Term)
    -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Decompose a triple term into exact subject/predicate/object keys.
    /// Return `None` only for a supported non-triple term; never assert its contents.
    fn triple_term(
        &self,
        term: &Self::Term,
    ) -> impl Future<Output = Result<Option<[Self::Term; 3]>, Self::Error>> + Send;
    /// Whether the enclosing immutable attempt is cancelled or superseded.
    fn is_cancelled(&self) -> bool;
}

/// Explicit bounds on target size, existential bindings, term worklist and work.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntailmentLimits {
    patterns: NonZeroU32,
    bindings: NonZeroU32,
    terms: NonZeroU32,
    steps: NonZeroU64,
}
impl EntailmentLimits {
    /// Bound retained target triples/active cursors, blank bindings, pending term
    /// pairs and total read/unification/probe steps. Source graphs remain streamed.
    /// Exceeding any cap is an error, never a negative entailment answer.
    pub const fn new(
        patterns: NonZeroU32,
        bindings: NonZeroU32,
        terms: NonZeroU32,
        steps: NonZeroU64,
    ) -> Self {
        Self {
            patterns,
            bindings,
            terms,
            steps,
        }
    }
}

/// Completed simple-entailment decision with bounded-work counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntailmentReport {
    /// Whether the source contains an instance of the entire target graph.
    pub entailed: bool,
    /// Target triples read before matching.
    pub target_triples: u32,
    /// Cursor reads, structural probes and term-pair unifications performed.
    pub steps: u64,
}

/// Incomplete entailment inspection; callers must not report `false` on error.
#[derive(Debug)]
pub enum EntailmentError<E> {
    /// Graph or term access failed.
    Storage(E),
    /// Caller cancelled or superseded the attempt.
    Cancelled,
    /// Retaining another target triple would exceed its explicit cap.
    TargetLimit,
    /// Another existential binding would exceed its explicit cap.
    BindingLimit,
    /// Pending recursive term pairs would exceed their explicit cap.
    TermLimit,
    /// The explicit total work-step budget was exhausted.
    StepLimit,
}
impl<E: fmt::Display> fmt::Display for EntailmentError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(f, "entailment storage failed: {error}"),
            Self::Cancelled => f.write_str("entailment cancelled"),
            Self::TargetLimit => f.write_str("entailment target limit exceeded"),
            Self::BindingLimit => f.write_str("entailment bindings limit exceeded"),
            Self::TermLimit => f.write_str("entailment term worklist limit exceeded"),
            Self::StepLimit => f.write_str("entailment steps limit exceeded"),
        }
    }
}
impl<E: Error + 'static> Error for EntailmentError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            _ => None,
        }
    }
}
