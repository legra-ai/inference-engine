use rdf_reasoning::RuleId;
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    num::{NonZeroU32, NonZeroU64},
    str::FromStr,
};

/// Precisely supported executable profile; distinct from compile-only presets.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ExecutionProfile {
    /// Only subclass transitivity and inherited membership, no axioms or other rules.
    #[serde(rename = "rdfs-subclass")]
    RdfsSubclass,
}

impl ExecutionProfile {
    /// Stable profile name; this is not full RDFS support.
    pub const fn name(self) -> &'static str {
        "rdfs-subclass"
    }
    /// Profile definition revision, independent of package implementation version.
    pub const fn revision(self) -> u32 {
        1
    }
    /// Exact rule identities from `rdf-reasoning` 2.6.0, in execution order.
    pub const fn rules(self) -> &'static [RuleId; 2] {
        &[RuleId::Rdfs9, RuleId::Rdfs11]
    }
}

impl FromStr for ExecutionProfile {
    type Err = ProfileError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "rdfs-subclass" => Ok(Self::RdfsSubclass),
            _ => Err(ProfileError(value.to_owned())),
        }
    }
}

/// Requested profile is not executable; no weaker profile was substituted.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProfileError(String);
impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unsupported executable profile: {}", self.0)
    }
}
impl Error for ProfileError {}

/// Explicit finite work/output limits; reaching a limit fails the attempt.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunLimits {
    pub(super) rounds: NonZeroU32,
    pub(super) inferred: NonZeroU64,
    pub(super) matches: NonZeroU64,
}
impl RunLimits {
    /// Set maximum rounds (including the final empty round), novel tuples and
    /// examined cursor rows. The caller separately budgets scratch disk/I/O.
    pub const fn new(rounds: NonZeroU32, inferred: NonZeroU64, matches: NonZeroU64) -> Self {
        Self {
            rounds,
            inferred,
            matches,
        }
    }
}

/// One novel tuple and a two-premise witness from the selected shared rule.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inference<T> {
    /// Derived subject, predicate and object, preserving caller term identity.
    pub triple: [T; 3],
    /// Rule that produced this witness.
    pub rule: RuleId,
    /// Matching body tuples in shared rule order; either can itself be inferred.
    pub premises: [[T; 3]; 2],
}

/// Completed work counters; returned only after successful fixpoint detection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunReport {
    /// Completed rounds, including the final round with no additions.
    pub rounds: u32,
    /// Examined cursor rows, including repeated probes across rounds.
    pub matches: u64,
    /// Novel inferred tuples, excluding asserted and inferred duplicates.
    pub inferred: u64,
}

/// Resource budget that prevented complete inference.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Limit {
    /// Round count reached before proving a fixpoint.
    Rounds,
    /// Maximum number of novel inferred tuples.
    Inferred,
    /// Maximum examined cursor rows.
    Matches,
}

/// Failure of the entire attempt; any staged output must remain unpublished.
#[derive(Debug)]
pub enum ReasoningError<E> {
    /// Fallible backend read, resolution or write failed.
    Storage(E),
    /// Caller cancelled or superseded the attempt.
    Cancelled,
    /// Explicit budget prevented complete closure.
    LimitExceeded(Limit),
    /// Pinned shared rule no longer has the supported flat two-body form.
    UnsupportedRule(RuleId),
}
impl<E: fmt::Display> fmt::Display for ReasoningError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(f, "reasoning storage failed: {error}"),
            Self::Cancelled => f.write_str("reasoning cancelled"),
            Self::LimitExceeded(limit) => write!(
                f,
                "reasoning {} limit exceeded",
                match limit {
                    Limit::Rounds => "rounds",
                    Limit::Inferred => "inferred",
                    Limit::Matches => "matches",
                }
            ),
            Self::UnsupportedRule(rule) => {
                write!(f, "unsupported shared rule definition: {rule:?}")
            }
        }
    }
}
impl<E: Error + 'static> Error for ReasoningError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            _ => None,
        }
    }
}
