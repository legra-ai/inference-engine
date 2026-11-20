//! SHACL 1.2 scheduling over an explicitly supplied native execution backend.
//!
//! This controller is not a SHACL compiler or a complete SHACL processor. The
//! backend must preflight supported rule types, expressions, conditions, targets,
//! processors and entailment selection before constructing a plan. It supplies
//! pinned graph scope, immutable query snapshots and queryable spilled staging.
//! All staging belongs to a fresh attempt; any error invalidates the attempt.
//! Only the caller can atomically publish successful output.

mod controller;
mod plan;
mod types;

pub use controller::Controller;
pub use plan::{Plan, PlanError, Rule};
pub use types::{ExecutionError, Limit, Limits, Report, Work, WorkExceeded};

use std::{error::Error, future::Future};

/// Native graph/query capabilities required by the scheduling controller.
///
/// This port extends beyond triple scans: a SPARQL evaluator may need an entire
/// pinned dataset context. Never silently flatten named graphs or copy a graph
/// into controller memory. Reuse canonical storage/query adapters. All callbacks
/// must observe cancellation and charge streamed reads, focus nodes, expression
/// work and candidate output through [`Work::charge`]. Dictionary, nesting, I/O,
/// and query-engine internal budgets remain mandatory backend obligations.
pub trait RuleBackend: Send {
    /// Identity/handle of a preflight-checked rule, unique across linked shapes.
    type Rule: Eq + Send + Sync;
    /// Owned immutable view: later staging and merges cannot change its results.
    type Snapshot: Send + Sync;
    /// Typed native failure, never translated into absence or an empty result.
    type Error: Error + Send + Sync + 'static;
    /// Freeze selected base plus the current attempt's inference graph.
    fn snapshot(&mut self) -> impl Future<Output = Result<Self::Snapshot, Self::Error>> + Send;
    /// Compute this layer's expected derived triples once before either phase.
    /// The backend knows the current layer from its ordered plan lifecycle.
    fn begin_layer(
        &mut self,
        work: &mut Work,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Execute one rule against exactly `snapshot`, staging into a separate phase.
    ///
    /// Global rules receive no focus binding. Shape rules stream the deduplicated
    /// union of active targets across linked shapes and test all active conditions.
    /// Preserve actual rule/focus/source/shapes/snapshot/phase evidence and fresh
    /// term identity. Do not forge RDFS proofs. `phase` is an attempt-local ordinal.
    /// No staged candidate may become visible to another execution in this phase.
    fn execute(
        &mut self,
        rule: &Self::Rule,
        snapshot: &Self::Snapshot,
        phase: u64,
        work: &mut Work,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Merge the complete pending phase; return only novel inferred triples.
    /// Deduplicate in indexed scratch, preserving assertion precedence and evidence.
    fn merge_phase(
        &mut self,
        work: &mut Work,
    ) -> impl Future<Output = Result<u64, Self::Error>> + Send;
    /// Remove this layer's derived-only triples and annotations; retain those
    /// independently inferred by rules. Called only after a proved fixed point.
    fn finish_layer(
        &mut self,
        work: &mut Work,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Remove temporary triples and their annotations, without publishing output.
    fn finish(&mut self, work: &mut Work) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Whether the pinned attempt has been cancelled or superseded.
    fn is_cancelled(&self) -> bool;
}
