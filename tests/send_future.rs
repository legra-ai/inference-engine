//! Backend stores need not be Sync to execute on a multithreaded runtime.

use inference_engine::entailment::EntailmentTerms;
use inference_engine::reasoning::{
    DatatypeConflictSink, LiteralOccurrenceCursor, LiteralRole, Rdf12AxiomSeeder,
    Rdf12DatatypeConflictChecker, Rdf12LiteralTyper, Rdf12OccurrenceCursor, Rdf12OccurrenceTyper,
    Rdf12PropositionConstructor, Rdf12TripleTermStore, Rdf12WitnessStore, Reasoner, ReasoningStore,
    TripleCursor,
};
use rdf_reasoning::Rdf12ExistentialAxiomId;
use std::future::Future;

fn require_send(_: impl Future + Send) {}

#[allow(dead_code)]
fn executor_future_is_send<S: ReasoningStore>(reasoner: &Reasoner, store: &mut S) {
    require_send(reasoner.run(store));
}

#[allow(dead_code)]
fn constructor_future_is_send<S: Rdf12TripleTermStore>(
    constructor: &Rdf12PropositionConstructor,
    store: &mut S,
) {
    require_send(constructor.run(store));
}

#[allow(dead_code)]
fn literal_typing_future_is_send<S, C>(typer: &Rdf12LiteralTyper, store: &mut S, cursor: &mut C)
where
    S: ReasoningStore,
    C: LiteralOccurrenceCursor<Term = S::Term, Error = S::Error>,
{
    require_send(typer.run(store, cursor));
}

#[allow(dead_code)]
fn witness_future_is_send<S: Rdf12WitnessStore>(seeder: &mut Rdf12AxiomSeeder, store: &mut S) {
    require_send(seeder.seed_existential(store, Rdf12ExistentialAxiomId::Rdfs14a));
}

#[allow(dead_code)]
fn conflicts_future_is_send<S, D>(
    checker: &Rdf12DatatypeConflictChecker,
    store: &mut S,
    sink: &mut D,
) where
    S: ReasoningStore,
    D: DatatypeConflictSink<Term = S::Term, Error = S::Error>,
{
    require_send(checker.run(store, LiteralRole::Source, sink));
}

#[allow(dead_code)]
fn occurrence_future_is_send<A, C>(cursor: &mut Rdf12OccurrenceCursor<'_, A, C>)
where
    A: EntailmentTerms,
    C: TripleCursor<Term = A::Term, Error = A::Error>,
{
    require_send(cursor.next());
}

#[allow(dead_code)]
fn occurrence_typing_future_is_send<S, A, C>(
    typer: &Rdf12OccurrenceTyper,
    store: &mut S,
    cursor: &mut Rdf12OccurrenceCursor<'_, A, C>,
) where
    S: ReasoningStore,
    A: EntailmentTerms<Term = S::Term, Error = S::Error>,
    C: TripleCursor<Term = S::Term, Error = S::Error>,
{
    require_send(typer.run(store, cursor));
}

#[allow(dead_code)]
fn borrowed_literal_error_retains_error_source(
    error: &inference_engine::reasoning::LiteralTypingError<&str, std::io::Error>,
) {
    let _ = std::error::Error::source(error);
}

#[allow(dead_code)]
fn candidate_constructor_future_is_send<S, C>(
    constructor: &Rdf12PropositionConstructor,
    store: &mut S,
    candidates: &mut C,
) where
    S: Rdf12TripleTermStore,
    C: TripleCursor<Term = S::Term, Error = S::Error>,
{
    require_send(constructor.run_candidates(store, candidates));
}

#[allow(dead_code)]
fn source_preparation_future_is_send<S, A, D>(
    source: &inference_engine::reasoning::Rdfs12Source,
    store: &mut S,
    access: &A,
    sink: &mut D,
) where
    S: Rdf12WitnessStore + Rdf12TripleTermStore,
    A: inference_engine::reasoning::LiteralTermAccess<Term = S::Term, Error = S::Error>
        + inference_engine::reasoning::Rdf12ContainerAccess,
    D: DatatypeConflictSink<Term = S::Term, Error = S::Error>,
{
    require_send(source.run(store, access, sink));
}
#[allow(dead_code)]
fn borrowed_source_error_retains_error_source(
    error: &inference_engine::reasoning::Rdfs12SourceError<&str, std::io::Error>,
) {
    let _ = std::error::Error::source(error);
}
