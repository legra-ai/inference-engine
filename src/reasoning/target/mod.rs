//! Finite target-selected construction over one immutable premise snapshot.
mod engine;
mod grounding;
use super::{PropositionLimits, PropositionReport, ReasoningError};
use crate::entailment::{EntailmentError, EntailmentLimits};
pub use engine::Rdf12TargetConstructor;
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    num::{NonZeroU32, NonZeroU64},
};

/// Explicit budgets for one target-construction pass; no implicit defaults.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetConstructionLimits {
    matching: EntailmentLimits,
    propositions: PropositionLimits,
    slots: NonZeroU32,
    work: NonZeroU64,
}
impl TargetConstructionLimits {
    /// Bound partial matching, total constructor work/output, pending grounding
    /// slots and total grounding steps. Backend dictionary/depth/I/O caps also apply.
    pub const fn new(
        matching: EntailmentLimits,
        propositions: PropositionLimits,
        slots: NonZeroU32,
        work: NonZeroU64,
    ) -> Self {
        Self {
            matching,
            propositions,
            slots,
            work,
        }
    }
}

/// Completed finite selection pass, never a full RDFS entailment verdict.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetConstructionReport {
    /// Consistent partial target substitutions examined, including the empty one.
    pub substitutions: u64,
    /// Source/target reads and unification/probe steps in partial matching.
    pub matching_steps: u64,
    /// Target inspection, recursive grounding and substitution work.
    pub grounding_steps: u64,
    /// Shared Grdfs14 candidate/premise work and novel proposition memberships.
    pub propositions: PropositionReport,
}

/// Failed construction invalidates the complete scratch attempt.
#[derive(Debug)]
pub enum TargetConstructionError<E> {
    /// Matching, target reads or structural access failed.
    Matching(EntailmentError<E>),
    /// Shared constructor, dictionary or premise access failed.
    Reasoning(ReasoningError<E>),
    /// Pending grounding tasks/values exceeded the configured cap.
    SlotLimit,
    /// The total target-inspection/grounding budget was exhausted.
    WorkLimit,
    /// A constructed key did not decompose to the exact requested tuple.
    Identity,
    /// A report counter could not be represented.
    CounterOverflow,
}
impl<E: fmt::Display> fmt::Display for TargetConstructionError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Matching(e) => write!(f, "target construction matching failed: {e}"),
            Self::Reasoning(e) => write!(f, "target construction failed: {e}"),
            Self::SlotLimit => f.write_str("target grounding slot limit exceeded"),
            Self::WorkLimit => f.write_str("target grounding work limit exceeded"),
            Self::Identity => f.write_str("target construction dictionary identity mismatch"),
            Self::CounterOverflow => f.write_str("target construction counter overflow"),
        }
    }
}
impl<E: Error + 'static> Error for TargetConstructionError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Matching(e) => Some(e),
            Self::Reasoning(e) => Some(e),
            _ => None,
        }
    }
}
impl<E> From<EntailmentError<E>> for TargetConstructionError<E> {
    fn from(e: EntailmentError<E>) -> Self {
        Self::Matching(e)
    }
}
impl<E> From<ReasoningError<E>> for TargetConstructionError<E> {
    fn from(e: ReasoningError<E>) -> Self {
        Self::Reasoning(e)
    }
}
