use super::{
    Rdf12ContainerAccess, Rdfs12SourceConfig, Rdfs12SourceError, Rdfs12SourceOutcome,
    Rdfs12SourceReport,
};
use crate::reasoning::{
    ExecutionProfile, LiteralRole, LiteralTermAccess, Rdf12AxiomSeeder,
    Rdf12DatatypeConflictChecker, Rdf12LiteralOccurrences, Rdf12LiteralTyper,
    Rdf12OccurrenceCursor, Rdf12OccurrenceTyper, Rdf12PropositionConstructor,
    Rdf12SourceDiagnosticSink, Rdf12TripleTermStore, Rdf12WitnessStore, Reasoner, ReasoningError,
    ReasoningStore, TripleCursor, TripleSource,
};
use rdf_reasoning::{Rdf12AxiomId, Rdf12ContainerProperty, Rdf12ExistentialAxiomId};
use std::marker::PhantomData;

/// Compose bounded source-only RDF/RDFS default-map preparation phases.
///
/// Start a fresh selected asserted snapshot; never expose prior closure or target
/// context as source input. All scratch remains caller-owned and unpublished.
/// Existing appearing triple terms are typed through the shared constructor;
/// arbitrary resource/property combinations are not generated. Target-relative
/// candidate completeness, goal scheduling and entailment decisions stay separate.
pub struct Rdfs12Source {
    pub(super) config: Rdfs12SourceConfig,
}
impl Rdfs12Source {
    /// Configure every phase explicitly; the serialized config has no defaults.
    pub const fn new(config: Rdfs12SourceConfig) -> Self {
        Self { config }
    }

    /// Prepare source axioms, inspected literals, occurrences, existing terms and core.
    ///
    /// First inspect every original source literal through successful EOF. Retain
    /// recognized lexical violations separately and return `Conflicting` without
    /// seeding or inference. Read/metadata/UTF-8/resource failures remain errors.
    /// Otherwise repeat bounded source rounds until no phase adds facts. Classify canonical
    /// container IRIs from actual direct/nested source appearances, not literal
    /// metadata. Seed distinct nonempty-class blanks through the caller's scope.
    /// Only then scan supported default-map conflicts into the separate sink.
    ///
    /// `Prepared` is not a source-to-target decision or a full consistency
    /// certificate. Check input currentness and the complete requested semantic
    /// contract before atomic promotion; `Conflicting` forbids promotion.
    ///
    /// # Errors
    /// Any compile/read/write/metadata/UTF-8/cancellation or resource failure
    /// invalidates the attempt. Discard scratch and retain diagnostics with scope.
    pub async fn run<S, A, D>(
        &self,
        store: &mut S,
        access: &A,
        sink: &mut D,
    ) -> Result<Rdfs12SourceOutcome, Rdfs12SourceError<S::Term, S::Error>>
    where
        S: Rdf12WitnessStore + Rdf12TripleTermStore,
        A: LiteralTermAccess<Term = S::Term, Error = S::Error> + Rdf12ContainerAccess,
        D: Rdf12SourceDiagnosticSink<Term = S::Term, Error = S::Error>,
    {
        let mut report = self.inspect_source(store, access, sink).await?;
        if report.diagnostics != 0 {
            return Ok(Rdfs12SourceOutcome::Conflicting(report));
        }
        let mut seeds = Rdf12AxiomSeeder::new(self.config.axioms);
        for family in [
            Rdf12AxiomId::Rdf,
            Rdf12AxiomId::Rdfs,
            Rdf12AxiomId::DefaultDatatypes,
        ] {
            seeds.seed_family(store, family).await?;
        }
        for &id in Rdf12ExistentialAxiomId::ALL {
            seeds.seed_existential(store, id).await?;
        }
        let first =
            Rdf12ContainerProperty::try_new("http://www.w3.org/1999/02/22-rdf-syntax-ns#_1")
                .expect("canonical standard container IRI");
        seeds.seed_container(store, &first).await?;
        report.seeded = seeds.inferred();
        report.inferred = seeds.inferred();
        loop {
            if report.rounds == self.config.rounds.get() {
                return Err(Rdfs12SourceError::Rounds);
            }
            report.rounds += 1;
            let before = report.inferred;
            self.vocabulary(store, access, &mut seeds).await?;
            Rdfs12SourceReport::add(&mut report.inferred, seeds.inferred() - report.seeded)?;
            report.seeded = seeds.inferred();
            self.literals(store, access, &mut report).await?;
            let roots = store
                .snapshot()
                .await
                .map_err(ReasoningError::Storage)?
                .scan([None, None, None])
                .await
                .map_err(ReasoningError::Storage)?;
            let mut cursor = Rdf12OccurrenceCursor::new(
                roots,
                access,
                LiteralRole::Source,
                self.config.traversal,
            );
            let typed =
                Rdf12OccurrenceTyper::new(self.config.structural.inferred, LiteralRole::Source)
                    .run(store, &mut cursor)
                    .await
                    .map_err(Rdfs12SourceError::Occurrences)?;
            Rdfs12SourceReport::add(&mut report.inferred, typed.inferred)?;
            self.propositions(store, access, &mut report).await?;
            let core = Reasoner::new(ExecutionProfile::RdfsCore, self.config.structural)
                .run(store)
                .await?;
            Rdfs12SourceReport::add(&mut report.inferred, core.inferred)?;
            if report.inferred == before {
                break;
            }
        }
        let conflicts = Rdf12DatatypeConflictChecker::new(self.config.conflicts)
            .run(
                store,
                LiteralRole::Source,
                &mut super::diagnostic::ConflictSink(sink),
            )
            .await
            .map_err(Rdfs12SourceError::Conflicts)?;
        report.diagnostics = conflicts.diagnostics;
        Ok(if report.diagnostics == 0 {
            Rdfs12SourceOutcome::Prepared(report)
        } else {
            Rdfs12SourceOutcome::Conflicting(report)
        })
    }
    async fn vocabulary<S, A>(
        &self,
        store: &mut S,
        access: &A,
        seeds: &mut Rdf12AxiomSeeder,
    ) -> Result<(), Rdfs12SourceError<S::Term, S::Error>>
    where
        S: ReasoningStore,
        A: Rdf12ContainerAccess<Term = S::Term, Error = S::Error>,
    {
        let roots = store
            .snapshot()
            .await
            .map_err(ReasoningError::Storage)?
            .scan([None, None, None])
            .await
            .map_err(ReasoningError::Storage)?;
        let mut cursor =
            Rdf12OccurrenceCursor::new(roots, access, LiteralRole::Source, self.config.traversal);
        while let Some(occurrence) = cursor.next().await.map_err(Rdfs12SourceError::Traversal)? {
            let descriptor = access
                .container_property(&occurrence.term, self.config.iri_bytes)
                .await
                .map_err(|error| Rdfs12SourceError::Vocabulary {
                    occurrence: occurrence.clone(),
                    error,
                })?;
            let Some(descriptor) = descriptor else {
                continue;
            };
            if descriptor.iri().len() > self.config.iri_bytes.get() as usize {
                return Err(Rdfs12SourceError::VocabularyLimit);
            }
            let resolved = store
                .resolve_iri(descriptor.iri())
                .await
                .map_err(ReasoningError::Storage)?;
            if resolved != occurrence.term {
                return Err(Rdfs12SourceError::VocabularyIdentity);
            }
            seeds.seed_container(store, &descriptor).await?;
        }
        Ok(())
    }
    async fn literals<S, A>(
        &self,
        store: &mut S,
        access: &A,
        report: &mut Rdfs12SourceReport,
    ) -> Result<(), Rdfs12SourceError<S::Term, S::Error>>
    where
        S: ReasoningStore,
        A: LiteralTermAccess<Term = S::Term, Error = S::Error>,
    {
        let roots = store
            .snapshot()
            .await
            .map_err(ReasoningError::Storage)?
            .scan([None, None, None])
            .await
            .map_err(ReasoningError::Storage)?;
        let mut cursor =
            Rdf12LiteralOccurrences::new(roots, access, LiteralRole::Source, self.config.traversal);
        let typed = Rdf12LiteralTyper::new(self.config.literals, LiteralRole::Source)
            .run(store, &mut cursor)
            .await
            .map_err(Rdfs12SourceError::Literals)?;
        Rdfs12SourceReport::add(&mut report.inferred, typed.inferred)?;
        Rdfs12SourceReport::add(&mut report.literals, typed.occurrences)?;
        Rdfs12SourceReport::add(&mut report.lexical_bytes, typed.bytes)
    }
    async fn propositions<S, A>(
        &self,
        store: &mut S,
        access: &A,
        report: &mut Rdfs12SourceReport,
    ) -> Result<(), Rdfs12SourceError<S::Term, S::Error>>
    where
        S: Rdf12TripleTermStore,
        A: crate::entailment::EntailmentTerms<Term = S::Term, Error = S::Error>,
    {
        let roots = store
            .snapshot()
            .await
            .map_err(ReasoningError::Storage)?
            .scan([None, None, None])
            .await
            .map_err(ReasoningError::Storage)?;
        let mut cursor =
            Rdf12OccurrenceCursor::new(roots, access, LiteralRole::Source, self.config.traversal);
        while let Some(occurrence) = cursor.next().await.map_err(Rdfs12SourceError::Traversal)? {
            let Some(triple) = access
                .triple_term(&occurrence.term)
                .await
                .map_err(ReasoningError::Storage)?
            else {
                continue;
            };
            let canonical = store
                .resolve_triple_term(triple.clone())
                .await
                .map_err(ReasoningError::Storage)?;
            if canonical != occurrence.term {
                return Err(Rdfs12SourceError::PropositionIdentity);
            }
            let mut candidate = Candidate::<_, S::Error> {
                triple: Some(triple),
                error: PhantomData,
            };
            let typed = Rdf12PropositionConstructor::new(self.config.propositions)
                .run_candidates(store, &mut candidate)
                .await?;
            Rdfs12SourceReport::add(&mut report.inferred, typed.inferred)?;
        }
        Ok(())
    }
}
struct Candidate<T, E> {
    triple: Option<[T; 3]>,
    error: PhantomData<fn() -> E>,
}
impl<T: Clone + Eq + Send + Sync, E: std::error::Error + Send + Sync + 'static> TripleCursor
    for Candidate<T, E>
{
    type Term = T;
    type Error = E;
    async fn next(&mut self) -> Result<Option<[T; 3]>, E> {
        Ok(self.triple.take())
    }
}
