use crate::reasoning::{
    DatatypeConflict, DatatypeConflictSink, LexicalViolation, LiteralOccurrence,
};
use serde::{Deserialize, Serialize};
use std::{error::Error, future::Future, num::NonZeroU64};

/// Non-RDF evidence about the selected source, never target augmentation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Rdf12SourceDiagnostic<T> {
    /// One member belongs to two disjoint recognized default datatype classes.
    DatatypeConflict(DatatypeConflict<T>),
    /// A recognized literal has no denotation under the selected default map.
    /// Its entire payload passed transport/UTF-8 inspection through EOF.
    IllTypedLiteral {
        /// Exact source key, original root and recursive occurrence path.
        occurrence: LiteralOccurrence<T>,
        /// Exact complete lexical byte count, without retaining the payload.
        lexical_bytes: u64,
        /// First violation reported by the shared lexical semantics.
        violation: LexicalViolation,
    },
}

/// Caller-owned streamed/spilled source diagnostics, separate from RDF scratch.
pub trait Rdf12SourceDiagnosticSink: Send {
    /// Exact source term identity.
    type Term: Clone + Eq + Send + Sync;
    /// Storage failure; losing a diagnostic aborts the operation.
    type Error: Error + Send + Sync + 'static;
    /// Stage one diagnostic with caller-owned source snapshot/graph provenance.
    /// Any later failure invalidates all partial diagnostics and inferred output.
    fn record(
        &mut self,
        diagnostic: Rdf12SourceDiagnostic<Self::Term>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// Explicit aggregate caps for source literal preflight, before inference.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLiteralLimits {
    pub(super) occurrences: NonZeroU64,
    pub(super) diagnostics: NonZeroU64,
    pub(super) bytes: u64,
}
impl SourceLiteralLimits {
    /// Set examined occurrences, semantic diagnostics and total lexical bytes.
    /// Zero bytes admits only empty payloads, each verified through an EOF probe.
    pub const fn new(occurrences: NonZeroU64, diagnostics: NonZeroU64, bytes: u64) -> Self {
        Self {
            occurrences,
            diagnostics,
            bytes,
        }
    }
}

pub(super) struct ConflictSink<'a, D>(pub &'a mut D);
impl<D: Rdf12SourceDiagnosticSink> DatatypeConflictSink for ConflictSink<'_, D> {
    type Term = D::Term;
    type Error = D::Error;
    async fn record(&mut self, conflict: DatatypeConflict<D::Term>) -> Result<(), D::Error> {
        self.0
            .record(Rdf12SourceDiagnostic::DatatypeConflict(conflict))
            .await
    }
}
