use super::{Bindings, DefinitionId, Pattern};
use crate::reasoning::{ReasoningError, ReasoningStore, TermOccurrence};
use datalog_structural_model::Literal;
use rdf_reasoning::{
    RDF12_APPEARING_TERM_RELATION, RDF12_APPEARING_TRIPLE_RELATION, Rdf12OccurrenceRuleId,
    TRIPLE_RELATION,
};

enum Premise<T> {
    Term(Pattern<T, 1>),
    Triple(Pattern<T>),
}
pub(in crate::reasoning) struct OccurrenceClause<T> {
    pub id: Rdf12OccurrenceRuleId,
    pub head: Pattern<T>,
    body: Premise<T>,
}
impl<T: Clone + Eq + Send + Sync> OccurrenceClause<T> {
    pub async fn compile<S: ReasoningStore<Term = T>>(
        id: Rdf12OccurrenceRuleId,
        store: &mut S,
    ) -> Result<Self, ReasoningError<S::Error>> {
        let rule = id.definition();
        let identity = DefinitionId::Occurrence(id);
        let [Literal::Positive(body)] = rule.body.as_slice() else {
            return Err(identity.error());
        };
        // bounded: one virtual premise and one fixed head bind at most three variables.
        let mut variables = Vec::with_capacity(3);
        let body = match id {
            Rdf12OccurrenceRuleId::RdfD2 => Premise::Triple(
                Pattern::compile(
                    body,
                    RDF12_APPEARING_TRIPLE_RELATION,
                    identity,
                    &mut variables,
                    store,
                )
                .await?,
            ),
            Rdf12OccurrenceRuleId::Rdfs4 => Premise::Term(
                Pattern::compile(
                    body,
                    RDF12_APPEARING_TERM_RELATION,
                    identity,
                    &mut variables,
                    store,
                )
                .await?,
            ),
        };
        let head =
            Pattern::compile(&rule.head, TRIPLE_RELATION, identity, &mut variables, store).await?;
        Ok(Self { id, head, body })
    }
    pub fn bind(&self, occurrence: &TermOccurrence<T>) -> Option<Bindings<T>> {
        let empty = std::array::from_fn(|_| None);
        match &self.body {
            Premise::Term(pattern) => pattern.bind(std::array::from_ref(&occurrence.term), empty),
            Premise::Triple(pattern) if occurrence.is_predicate() => {
                pattern.bind(&occurrence.parent, empty)
            }
            Premise::Triple(_) => None,
        }
    }
}
