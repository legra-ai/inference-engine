use super::{AxiomFamily, Evidence, Inference, Limit, ReasoningError, ReasoningStore};
use datalog_structural_model::{Rule, Term};
use rdf_reasoning::{Rdf12AxiomId, Rdf12ContainerProperty, TRIPLE_RELATION};
use std::num::NonZeroU64;

/// Explicit bounded seeding of shared RDF 1.2 axioms into fresh attempt storage.
///
/// This is independent of structural-rule profile selection and does not claim
/// full RDFS entailment. The caller chooses source/target-relevant container
/// vocabulary; this type never enumerates infinite `rdf:_n` IRIs or scans a graph.
/// Select `_1` where required by the standard's target-entailment procedure.
///
/// One instance budgets all its seed calls cumulatively. Rule execution has its
/// own output budget; callers must also enforce an attempt-wide scratch budget.
/// On any failure discard the attempt, including all successfully staged seeds.
pub struct Rdf12AxiomSeeder {
    maximum: NonZeroU64,
    inferred: u64,
}

impl Rdf12AxiomSeeder {
    /// Set the maximum total novel axiom tuples across this seeder's calls.
    pub const fn new(maximum: NonZeroU64) -> Self {
        Self {
            maximum,
            inferred: 0,
        }
    }

    /// Total newly staged axiom tuples; asserted/inferred duplicates are excluded.
    pub const fn inferred(&self) -> u64 {
        self.inferred
    }

    /// Stage one finite shared family, preserving exact family and ground tuple.
    ///
    /// # Errors
    /// Storage failure, cancellation, unsupported definitions or exhausted output
    /// budget abort the attempt. Successful prior writes remain unpublished scratch.
    pub async fn seed_family<S: ReasoningStore>(
        &mut self,
        store: &mut S,
        family: Rdf12AxiomId,
    ) -> Result<(), ReasoningError<S::Error>> {
        Self::check_cancelled(store)?;
        // bounded: each fixed standard family has at most 41 ground definitions.
        for rule in family.definitions() {
            self.seed_rule(store, &rule, AxiomFamily::Rdf12(family))
                .await?;
        }
        Self::check_cancelled(store)
    }

    /// Stage the four shared RDF/RDFS axioms for one already checked container IRI.
    ///
    /// The caller supplies relevant vocabulary incrementally, including target
    /// vocabulary when checking entailment. Merely seeding observed input IRIs is
    /// insufficient to implement arbitrary source-to-target entailment.
    ///
    /// # Errors
    /// Storage failure, cancellation, unsupported definitions or exhausted output
    /// budget abort the entire attempt; partial seeds must never be promoted.
    pub async fn seed_container<S: ReasoningStore, I: AsRef<str>>(
        &mut self,
        store: &mut S,
        container: &Rdf12ContainerProperty<I>,
    ) -> Result<(), ReasoningError<S::Error>> {
        Self::check_cancelled(store)?;
        self.seed_rule(
            store,
            &container.rdf_definition(),
            AxiomFamily::Rdf12Container,
        )
        .await?;
        for rule in container.rdfs_definitions() {
            self.seed_rule(store, &rule, AxiomFamily::Rdf12Container)
                .await?;
        }
        Self::check_cancelled(store)
    }

    async fn seed_rule<S: ReasoningStore>(
        &mut self,
        store: &mut S,
        rule: &Rule,
        family: AxiomFamily,
    ) -> Result<(), ReasoningError<S::Error>> {
        Self::check_cancelled(store)?;
        let [Term::Constant(s), Term::Constant(p), Term::Constant(o)] = rule.head.terms.as_slice()
        else {
            return Err(ReasoningError::UnsupportedAxiom);
        };
        if !rule.body.is_empty() || rule.head.predicate.as_str() != TRIPLE_RELATION {
            return Err(ReasoningError::UnsupportedAxiom);
        }
        let triple = [
            store
                .resolve_iri(s.as_str())
                .await
                .map_err(ReasoningError::Storage)?,
            store
                .resolve_iri(p.as_str())
                .await
                .map_err(ReasoningError::Storage)?,
            store
                .resolve_iri(o.as_str())
                .await
                .map_err(ReasoningError::Storage)?,
        ];
        if store
            .contains(&triple)
            .await
            .map_err(ReasoningError::Storage)?
        {
            return Self::check_cancelled(store);
        }
        Self::check_cancelled(store)?;
        if self.inferred == self.maximum.get() {
            return Err(ReasoningError::LimitExceeded(Limit::Inferred));
        }
        if store
            .insert_inferred(Inference {
                triple,
                evidence: Evidence::Axiom(family),
            })
            .await
            .map_err(ReasoningError::Storage)?
        {
            self.inferred += 1;
        }
        Self::check_cancelled(store)
    }

    fn check_cancelled<S: ReasoningStore>(store: &S) -> Result<(), ReasoningError<S::Error>> {
        if store.is_cancelled() {
            Err(ReasoningError::Cancelled)
        } else {
            Ok(())
        }
    }
}
