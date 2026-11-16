use super::LiteralOccurrence;
use rdf_reasoning::{Exclusion, Preset, Rdf12AxiomId, Rdf12LiteralRuleId, RuleId};
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
    /// Fourteen shared RDF/RDFS rules, without axioms or datatype processing.
    #[serde(rename = "rdfs-core")]
    RdfsCore,
}

impl ExecutionProfile {
    /// Stable profile name; this is not full RDFS support.
    pub const fn name(self) -> &'static str {
        self.preset().name()
    }
    /// Profile definition revision, independent of package implementation version.
    pub const fn revision(self) -> u32 {
        1
    }
    /// Shared semantic selection, independent of this executor's implementation.
    pub const fn preset(self) -> Preset {
        match self {
            Self::RdfsSubclass => Preset::RdfsSubclass,
            Self::RdfsCore => Preset::RdfsCore,
        }
    }
    /// Exact rule identities from the shared catalogue, in execution order.
    pub const fn rules(self) -> &'static [RuleId] {
        self.preset().rules()
    }
    /// Obligations deliberately excluded by the shared semantic selection.
    pub const fn exclusions(self) -> &'static [Exclusion] {
        self.preset().exclusions()
    }
}

impl FromStr for ExecutionProfile {
    type Err = ProfileError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "rdfs-subclass" => Ok(Self::RdfsSubclass),
            "rdfs-core" => Ok(Self::RdfsCore),
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

/// Exact bounded witness arity, without invented or repeated premise tuples.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Premises<T> {
    /// A unary rule used one matching body tuple.
    One([T; 3]),
    /// A join rule used two matching body tuples in shared definition order.
    Two([[T; 3]; 2]),
}
impl<T> Premises<T> {
    /// Iterate matching body tuples in rule order, preserving exact arity.
    pub fn iter(&self) -> std::slice::Iter<'_, [T; 3]> {
        match self {
            Self::One(tuple) => std::slice::from_ref(tuple).iter(),
            Self::Two(tuples) => tuples.iter(),
        }
    }
}
impl<T> From<[[T; 3]; 2]> for Premises<T> {
    fn from(tuples: [[T; 3]; 2]) -> Self {
        Self::Two(tuples)
    }
}

/// Standard axiom authority; the inference tuple supplies the exact ground identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AxiomFamily {
    /// A finite family from RDF 1.2 Semantics CRD 2026-09-24.
    Rdf12(Rdf12AxiomId),
    /// A checked canonical container IRI's RDF/RDFS vocabulary axioms.
    Rdf12Container,
}

/// Exact derivation authority, distinguishing premises from ground axioms.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Evidence<T> {
    /// A shared rule fired on these exact body tuples.
    Rule {
        /// Catalogue rule identity.
        rule: RuleId,
        /// Body tuples in shared definition order.
        premises: Premises<T>,
    },
    /// A ground standard axiom has no rule premises.
    Axiom(AxiomFamily),
    /// A shared literal-typing clause consumed validated semantic metadata.
    Literal {
        /// Exact parameterized shared clause identity.
        rule: Rdf12LiteralRuleId,
        /// Original selected root and literal key, including source/target role.
        occurrence: LiteralOccurrence<T>,
        /// Exact validated lexical byte count; not a lexical value replacement.
        lexical_bytes: u64,
    },
}

/// One novel tuple and its exact derivation authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inference<T> {
    /// Derived subject, predicate and object, preserving caller term identity.
    pub triple: [T; 3],
    /// Rule and premise witness, or explicit standard axiom family.
    pub evidence: Evidence<T>,
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
    /// Shared rule does not have a supported flat one- or two-body form.
    UnsupportedRule(RuleId),
    /// A shared axiom is not an empty-body ground triple of IRI constants.
    UnsupportedAxiom,
    /// A shared literal clause does not have the admitted metadata-premise shape.
    UnsupportedLiteralRule(Rdf12LiteralRuleId),
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
            Self::UnsupportedAxiom => f.write_str("unsupported shared ground axiom definition"),
            Self::UnsupportedLiteralRule(rule) => {
                write!(f, "unsupported shared literal rule definition: {rule:?}")
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
