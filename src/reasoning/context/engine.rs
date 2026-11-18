use super::{Rdfs12ContextConfig, Rdfs12ContextError, Rdfs12ContextOutcome, Rdfs12TargetReport};
use crate::reasoning::{
    DatatypeConflictSink, ExecutionProfile, LiteralRole, LiteralTermAccess, Rdf12AxiomSeeder,
    Rdf12ContainerAccess, Rdf12LiteralOccurrences, Rdf12LiteralTyper, Rdf12OccurrenceCursor,
    Rdf12OccurrenceTyper, Rdf12TripleTermStore, Rdf12WitnessStore, Rdfs12Source,
    Rdfs12SourceOutcome, Reasoner, ReasoningError, ReasoningStore, TripleSource,
};

/// Complete selected source phases before augmenting original-target context.
///
/// This controller neither asserts target triples nor executes source predicate
/// typing on target terms. It does not construct adequately substituted target
/// propositions or decide full RDFS entailment/consistency. Missing matches stay
/// undecided outside a complete semantic procedure.
pub struct Rdfs12Context {
    config: Rdfs12ContextConfig,
}
impl Rdfs12Context {
    /// Configure source and target budgets separately, without implicit defaults.
    pub const fn new(config: Rdfs12ContextConfig) -> Self {
        Self { config }
    }

    /// Prepare a fresh selected source and then isolated original-target context.
    ///
    /// The immutable target and selected source must have independent blank scopes
    /// and share the accessor/store's exact append-only term dictionary. All reads
    /// must use authoritative persisted input; cache misses reload or fail. Caller
    /// owns freshness, attempt-wide storage/I/O bounds, provenance and promotion.
    /// Source diagnostics finish before the first target scan; supported source
    /// conflicts return immediately. Target facts are never reclassified as source
    /// input or fed into source consistency. Preserve target-role proof roots/paths.
    ///
    /// # Errors
    /// Any source or target read, lexical, identity, cancellation or budget failure
    /// invalidates all scratch. A known-invalid target literal remains a target
    /// error, never a source-conflict or negative-entailment verdict.
    pub async fn run<S, A, T, D>(
        &self,
        store: &mut S,
        access: &A,
        target: &T,
        sink: &mut D,
    ) -> Result<Rdfs12ContextOutcome, Rdfs12ContextError<S::Term, S::Error>>
    where
        S: Rdf12WitnessStore + Rdf12TripleTermStore,
        A: LiteralTermAccess<Term = S::Term, Error = S::Error> + Rdf12ContainerAccess,
        T: TripleSource<Term = S::Term, Error = S::Error>,
        D: DatatypeConflictSink<Term = S::Term, Error = S::Error>,
    {
        let source = match Rdfs12Source::new(self.config.source)
            .run(store, access, sink)
            .await
            .map_err(Rdfs12ContextError::Source)?
        {
            Rdfs12SourceOutcome::Prepared(report) => report,
            Rdfs12SourceOutcome::Conflicting(report) => {
                return Ok(Rdfs12ContextOutcome::SourceConflicting(report));
            }
        };
        let mut seeds = Rdf12AxiomSeeder::new(self.config.target.axioms);
        self.vocabulary(store, access, target, &mut seeds).await?;
        let roots = target
            .scan([None, None, None])
            .await
            .map_err(ReasoningError::Storage)?;
        let mut literals = Rdf12LiteralOccurrences::new(
            roots,
            access,
            LiteralRole::TargetContext,
            self.config.target.traversal,
        );
        let literals =
            Rdf12LiteralTyper::new(self.config.target.literals, LiteralRole::TargetContext)
                .run(store, &mut literals)
                .await
                .map_err(Rdfs12ContextError::Literals)?;
        let roots = target
            .scan([None, None, None])
            .await
            .map_err(ReasoningError::Storage)?;
        let mut terms = Rdf12OccurrenceCursor::new(
            roots,
            access,
            LiteralRole::TargetContext,
            self.config.target.traversal,
        );
        let terms = Rdf12OccurrenceTyper::new(
            self.config.target.structural.inferred,
            LiteralRole::TargetContext,
        )
        .run(store, &mut terms)
        .await
        .map_err(Rdfs12ContextError::Occurrences)?;
        let core = Reasoner::new(ExecutionProfile::RdfsCore, self.config.target.structural)
            .run(store)
            .await?;
        let inferred = seeds
            .inferred()
            .checked_add(literals.inferred)
            .and_then(|value| value.checked_add(terms.inferred))
            .and_then(|value| value.checked_add(core.inferred))
            .ok_or(Rdfs12ContextError::CounterOverflow)?;
        Ok(Rdfs12ContextOutcome::Prepared {
            source,
            target: Rdfs12TargetReport {
                seeded: seeds.inferred(),
                inferred,
                occurrences: terms.occurrences,
                deferred_triple_terms: terms.deferred_triple_terms,
                literals: literals.occurrences,
                lexical_bytes: literals.bytes,
                core,
            },
        })
    }
    async fn vocabulary<S, A, T>(
        &self,
        store: &mut S,
        access: &A,
        target: &T,
        seeds: &mut Rdf12AxiomSeeder,
    ) -> Result<(), Rdfs12ContextError<S::Term, S::Error>>
    where
        S: ReasoningStore,
        A: Rdf12ContainerAccess<Term = S::Term, Error = S::Error>,
        T: TripleSource<Term = S::Term, Error = S::Error>,
    {
        let roots = target
            .scan([None, None, None])
            .await
            .map_err(ReasoningError::Storage)?;
        let mut cursor = Rdf12OccurrenceCursor::new(
            roots,
            access,
            LiteralRole::TargetContext,
            self.config.target.traversal,
        );
        while let Some(occurrence) = cursor.next().await.map_err(Rdfs12ContextError::Traversal)? {
            let descriptor = access
                .container_property(&occurrence.term, self.config.target.iri_bytes)
                .await
                .map_err(|error| Rdfs12ContextError::Vocabulary {
                    occurrence: occurrence.clone(),
                    error,
                })?;
            let Some(descriptor) = descriptor else {
                continue;
            };
            if descriptor.iri().len() > self.config.target.iri_bytes.get() as usize {
                return Err(Rdfs12ContextError::VocabularyLimit);
            }
            let resolved = store
                .resolve_iri(descriptor.iri())
                .await
                .map_err(ReasoningError::Storage)?;
            if resolved != occurrence.term {
                return Err(Rdfs12ContextError::VocabularyIdentity);
            }
            seeds.seed_container(store, &descriptor).await?;
        }
        Ok(())
    }
}
