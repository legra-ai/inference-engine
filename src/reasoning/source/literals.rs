use super::{
    Rdf12SourceDiagnostic, Rdf12SourceDiagnosticSink, Rdfs12Source, Rdfs12SourceError,
    Rdfs12SourceReport,
};
use crate::reasoning::{
    LiteralOccurrenceCursor, LiteralRole, LiteralTermAccess, LiteralTypingError,
    Rdf12LiteralOccurrences, Rdf12LiteralValidator, ReasoningError, ReasoningStore, TripleSource,
};

impl Rdfs12Source {
    pub(super) async fn inspect_source<S, A, D>(
        &self,
        store: &mut S,
        access: &A,
        sink: &mut D,
    ) -> Result<Rdfs12SourceReport, Rdfs12SourceError<S::Term, S::Error>>
    where
        S: ReasoningStore,
        A: LiteralTermAccess<Term = S::Term, Error = S::Error>,
        D: Rdf12SourceDiagnosticSink<Term = S::Term, Error = S::Error>,
    {
        Self::check_source_cancelled(store)?;
        let roots = store
            .snapshot()
            .await
            .map_err(ReasoningError::Storage)?
            .scan([None, None, None])
            .await
            .map_err(ReasoningError::Storage)?;
        let mut cursor =
            Rdf12LiteralOccurrences::new(roots, access, LiteralRole::Source, self.config.traversal);
        let limits = self.config.inspection;
        let mut report = Rdfs12SourceReport::default();
        loop {
            Self::check_source_cancelled(store)?;
            let next = cursor
                .next()
                .await
                .map_err(|error| Rdfs12SourceError::Literals(LiteralTypingError::Cursor(error)))?;
            Self::check_source_cancelled(store)?;
            let Some((occurrence, mut reader)) = next else {
                return Ok(report);
            };
            if report.literals == limits.occurrences.get() {
                return Err(Rdfs12SourceError::Literals(
                    LiteralTypingError::OccurrenceLimit,
                ));
            }
            report.literals += 1;
            crate::cooperative::checkpoint(report.literals).await;
            Self::check_source_cancelled(store)?;
            let inspection = Rdf12LiteralValidator::new(limits.bytes - report.lexical_bytes)
                .inspect(&mut reader)
                .await
                .map_err(|error| {
                    Rdfs12SourceError::Literals(LiteralTypingError::Literal {
                        occurrence: occurrence.clone(),
                        error,
                    })
                })?;
            Self::check_source_cancelled(store)?;
            report.lexical_bytes += inspection.report.bytes;
            if let Some(violation) = inspection.violation {
                if report.diagnostics == limits.diagnostics.get() {
                    return Err(Rdfs12SourceError::DiagnosticLimit);
                }
                sink.record(Rdf12SourceDiagnostic::IllTypedLiteral {
                    occurrence,
                    lexical_bytes: inspection.report.bytes,
                    violation,
                })
                .await
                .map_err(ReasoningError::Storage)?;
                Self::check_source_cancelled(store)?;
                report.diagnostics += 1;
            }
        }
    }
    fn check_source_cancelled<S: ReasoningStore>(
        store: &S,
    ) -> Result<(), Rdfs12SourceError<S::Term, S::Error>> {
        if store.is_cancelled() {
            Err(ReasoningError::Cancelled.into())
        } else {
            Ok(())
        }
    }
}
