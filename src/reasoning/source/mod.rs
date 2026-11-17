//! Source-only default-map preparation before target-relative entailment context.
mod engine;
mod error;
use super::{
    DatatypeConflictLimits, LiteralTypingLimits, OccurrenceLimits, PropositionLimits, RunLimits,
};
use crate::entailment::EntailmentTerms;
pub use engine::Rdfs12Source;
pub use error::Rdfs12SourceError;
use rdf_reasoning::Rdf12ContainerProperty;
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    num::{NonZeroU32, NonZeroU64},
};

/// Authoritative, bounded classification of canonical container-property IRIs.
pub trait Rdf12ContainerAccess: EntailmentTerms {
    /// Borrowed or shared bounded IRI metadata, without requiring a string clone.
    type ContainerIri<'a>: AsRef<str> + Send + Sync
    where
        Self: 'a;
    /// Return a shared checked descriptor only for this exact appearing IRI key.
    /// Return `None` for a supported noncontainer term; cache misses reload or fail.
    /// Bound owned IRI metadata before allocation with `maximum_iri_bytes`, and
    /// report unsupported access/resource failures explicitly through the backend.
    fn container_property<'a>(
        &'a self,
        term: &Self::Term,
        maximum_iri_bytes: NonZeroU32,
    ) -> impl Future<
        Output = Result<Option<Rdf12ContainerProperty<Self::ContainerIri<'a>>>, Self::Error>,
    > + Send;
}

/// Explicit serialized resource configuration; no implicit budget defaults.
///
/// Phase budgets apply per source round, except cumulative axiom seeding.
/// The round cap therefore also bounds total phase work. Backends separately
/// enforce attempt-wide scratch, dictionary, I/O and metadata budgets.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rdfs12SourceConfig {
    /// Cumulative novel ground, container and nonempty-witness axioms.
    pub axioms: NonZeroU64,
    /// Source rounds, including the final round proving no additional facts.
    pub rounds: NonZeroU32,
    /// Shared core fixpoint work/output caps for each round.
    pub structural: RunLimits,
    /// Depth, appearances and work caps for each immutable traversal.
    pub traversal: OccurrenceLimits,
    /// Literal occurrence/output/lexical-byte caps per round.
    pub literals: LiteralTypingLimits,
    /// Work/output caps for each existing-term candidate check.
    pub propositions: PropositionLimits,
    /// Complete source-only diagnostic phase budgets.
    pub conflicts: DatatypeConflictLimits,
    /// Maximum owned metadata bytes for one classified container IRI.
    pub iri_bytes: NonZeroU32,
}

/// Completed source preparation counters; not a source-to-target decision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rdfs12SourceReport {
    /// Completed source rounds, including the final round with no new facts.
    pub rounds: u32,
    /// Novel ground/container/nonempty-witness axioms across the whole operation.
    pub seeded: u64,
    /// Total novel facts across seeding and all executed phases.
    pub inferred: u64,
    /// Literal appearances inspected, including repeated derived occurrences.
    pub literals: u64,
    /// Aggregate streamed lexical bytes, including repeated inspections.
    pub lexical_bytes: u64,
    /// Supported default-map conflicts retained separately from RDF output.
    pub diagnostics: u64,
}
impl Rdfs12SourceReport {
    fn add<T, E>(target: &mut u64, count: u64) -> Result<(), Rdfs12SourceError<T, E>> {
        *target = target
            .checked_add(count)
            .ok_or(Rdfs12SourceError::CounterOverflow)?;
        Ok(())
    }
}

/// Outcome of the selected source phases; neither variant is an entailment verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Rdfs12SourceOutcome {
    /// Source phases completed without these supported datatype conflicts.
    /// Full target-relative construction/matching and unsupported semantics remain separate.
    Prepared(Rdfs12SourceReport),
    /// Supported source conflicts were retained; do not promote the scratch graph.
    Conflicting(Rdfs12SourceReport),
}
