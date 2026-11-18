use super::{
    TargetConstructionError, TargetConstructionLimits, TargetConstructionReport,
    grounding::Grounding,
};
use crate::{
    entailment::{EntailmentTerms, PartialMatches},
    reasoning::{Rdf12PropositionConstructor, Rdf12TripleTermStore, ReasoningError, TripleSource},
};

/// Select substituted original-target triple terms for shared Grdfs14 construction.
///
/// Run after ordered source/context preparation. This is one finite positive-search
/// phase, not a complete RDFS decision procedure. Newly staged facts cannot feed
/// matching or constructor premises until the caller starts another pass, normally
/// after core closure. Never feed it an unrestricted resource/property product.
pub struct Rdf12TargetConstructor {
    limits: TargetConstructionLimits,
}
impl Rdf12TargetConstructor {
    /// Select explicit matching, grounding and constructor budgets.
    pub const fn new(limits: TargetConstructionLimits) -> Self {
        Self { limits }
    }

    /// Stream consistent partial matches, substitute original target triple terms,
    /// and validate all three shared typing premises in one fixed snapshot.
    ///
    /// Matched subsets supply candidate bindings; skipped patterns are not proved.
    /// Unbound variables receive the single `rdf:type` representative, consistently
    /// within each substitution. This is a selected witness search, not an adequate
    /// enumeration of all RDFS interpretations. Constructor premises still gate
    /// every fact, and a separate whole-target match is required for a proof.
    ///
    /// Source, target, accessor and store must share exact append-only dictionary
    /// identities and cancellation, with independently scoped target blanks. Keep
    /// original source/target snapshots, phase snapshot, profile/catalogue and graph
    /// provenance beside shared constructor evidence. Target contents are never
    /// asserted. Actual substituted depth and dictionary growth are backend-bounded.
    ///
    /// # Errors
    /// Any exhausted budget, cancellation, read/write or identity failure aborts
    /// the entire attempt. Zero output never establishes negative entailment.
    pub async fn run<S, A, T>(
        &self,
        store: &mut S,
        access: &A,
        target: &T,
    ) -> Result<TargetConstructionReport, TargetConstructionError<S::Error>>
    where
        S: Rdf12TripleTermStore,
        A: EntailmentTerms<Term = S::Term, Error = S::Error>,
        T: TripleSource<Term = S::Term, Error = S::Error>,
    {
        let mut phase = Rdf12PropositionConstructor::new(self.limits.propositions)
            .begin(store)
            .await?;
        let mut matches =
            PartialMatches::new(phase.source(), target, access, self.limits.matching).await?;
        let mut grounding = Grounding::new(self.limits);
        let mut report = TargetConstructionReport::default();
        if !grounding
            .has_goals(store, access, matches.patterns())
            .await?
        {
            report.matching_steps = matches.steps();
            report.grounding_steps = grounding.steps();
            return Ok(report);
        }
        let representative = store
            .resolve_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
            .await
            .map_err(ReasoningError::Storage)?;
        while matches.next(phase.source(), access).await? {
            report.substitutions = report
                .substitutions
                .checked_add(1)
                .ok_or(TargetConstructionError::CounterOverflow)?;
            for pattern in matches.patterns() {
                for term in pattern {
                    grounding
                        .run(
                            store,
                            access,
                            term,
                            matches.bindings(),
                            &representative,
                            &mut phase,
                        )
                        .await?;
                }
            }
        }
        grounding.finish(store, access).await?;
        report.matching_steps = matches.steps();
        report.grounding_steps = grounding.steps();
        report.propositions = phase.report;
        Ok(report)
    }
}
