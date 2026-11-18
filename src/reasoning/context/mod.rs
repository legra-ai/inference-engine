//! Source preparation followed by isolated target-relative context.
mod engine;
mod error;
use super::{
    LiteralTypingLimits, OccurrenceLimits, Rdfs12SourceConfig, Rdfs12SourceReport, RunLimits,
    RunReport,
};
pub use engine::Rdfs12Context;
pub use error::Rdfs12ContextError;
use serde::{Deserialize, Serialize};
use std::num::{NonZeroU32, NonZeroU64};

/// Explicit resource limits for one immutable target-context augmentation.
/// Backend attempt-wide scratch, dictionary, metadata and I/O limits also apply.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rdfs12TargetConfig {
    /// Cumulative novel target-vocabulary container axioms.
    pub axioms: NonZeroU64,
    /// Depth, appearances and work per traversal of the original target.
    pub traversal: OccurrenceLimits,
    /// Target literal occurrence, novel output and streamed lexical-byte caps.
    pub literals: LiteralTypingLimits,
    /// Core fixpoint limits and the target occurrence phase's novel-output cap.
    pub structural: RunLimits,
    /// Maximum owned metadata bytes for one container descriptor.
    pub iri_bytes: NonZeroU32,
}

/// Separate explicit source and target-context resource configurations.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rdfs12ContextConfig {
    /// Source-only preparation and diagnostic budgets.
    pub source: Rdfs12SourceConfig,
    /// Original-target augmentation and subsequent core budgets.
    pub target: Rdfs12TargetConfig,
}

/// Completed target-context phase counters, never an entailment verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rdfs12TargetReport {
    /// Novel container-vocabulary axioms from actual recursive target appearances.
    pub seeded: u64,
    /// Novel facts from all target-context phases, including core consequences.
    pub inferred: u64,
    /// Original target term appearances inspected for resource typing.
    pub occurrences: u64,
    /// Original target literal appearances inspected, including duplicates.
    pub literals: u64,
    /// Exact streamed lexical bytes across those literal appearances.
    pub lexical_bytes: u64,
    /// Completed shared core fixpoint after the original-target phases.
    pub core: RunReport,
}

/// Source/context preparation result; no negative or full-consistency decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Rdfs12ContextOutcome {
    /// Source phases and original-target augmentation completed.
    /// Goal-relevant construction and the full entailment procedure remain separate.
    Prepared {
        /// Completed source-only work and supported diagnostic count.
        source: Rdfs12SourceReport,
        /// Completed target-context work, separate from source diagnostics.
        target: Rdfs12TargetReport,
    },
    /// Supported source conflicts were retained before reading any target rows.
    /// Do not promote scratch or interpret this as an entailment verdict.
    SourceConflicting(Rdfs12SourceReport),
}
