mod occurrence;
use super::{ReasoningError, ReasoningStore};
use datalog_structural_model::{Atom, Literal, Term, VariableSymbol};
pub(super) use occurrence::OccurrenceClause;
use rdf_reasoning::{
    RDF12_LITERAL_DATATYPE_RELATION, Rdf12DatatypeConflictId, Rdf12LiteralRuleId,
    Rdf12OccurrenceRuleId, Rdf12PropositionRuleId, RuleId, TRIPLE_RELATION,
};

const VARIABLES: usize = 6;
pub(super) type Bindings<T> = [Option<T>; VARIABLES];

pub(super) enum Slot<T> {
    Constant(T),
    Variable(usize),
}
pub(super) struct Pattern<T, const N: usize = 3>([Slot<T>; N]);
#[derive(Clone, Copy)]
enum DefinitionId {
    Rdf(RuleId),
    Literal(Rdf12LiteralRuleId),
    Proposition(Rdf12PropositionRuleId),
    Conflict(Rdf12DatatypeConflictId),
    Occurrence(Rdf12OccurrenceRuleId),
}
impl DefinitionId {
    fn error<E>(self) -> ReasoningError<E> {
        match self {
            Self::Rdf(id) => ReasoningError::UnsupportedRule(id),
            Self::Literal(id) => ReasoningError::UnsupportedLiteralRule(id),
            Self::Proposition(id) => ReasoningError::UnsupportedPropositionRule(id),
            Self::Conflict(id) => ReasoningError::UnsupportedDatatypeConflict(id),
            Self::Occurrence(id) => ReasoningError::UnsupportedOccurrenceRule(id),
        }
    }
}
pub(super) struct Clause<T> {
    pub id: RuleId,
    pub head: Pattern<T>,
    pub body: Body<T>,
}

pub(super) struct PropositionClause<T> {
    pub id: Rdf12PropositionRuleId,
    pub body: [Pattern<T>; 3],
    pub triple: Pattern<T>,
    pub predicate: T,
    pub object: T,
}
impl<T: Clone + Eq + Send + Sync> PropositionClause<T> {
    pub async fn compile<S: ReasoningStore<Term = T>>(
        id: Rdf12PropositionRuleId,
        store: &mut S,
    ) -> Result<Self, ReasoningError<S::Error>> {
        let rule = id.definition();
        let identity = DefinitionId::Proposition(id);
        let [
            Literal::Positive(first),
            Literal::Positive(second),
            Literal::Positive(third),
        ] = rule.body.as_slice()
        else {
            return Err(identity.error());
        };
        let [
            Term::TripleTerm(quoted),
            Term::Constant(predicate),
            Term::Constant(object),
        ] = rule.head.terms.as_slice()
        else {
            return Err(identity.error());
        };
        if rule.head.predicate.as_str() != TRIPLE_RELATION {
            return Err(identity.error());
        }
        // bounded: three fixed body atoms and at most six admitted variable slots.
        let mut variables = Vec::with_capacity(VARIABLES);
        let body = [
            Pattern::compile(first, TRIPLE_RELATION, identity, &mut variables, store).await?,
            Pattern::compile(second, TRIPLE_RELATION, identity, &mut variables, store).await?,
            Pattern::compile(third, TRIPLE_RELATION, identity, &mut variables, store).await?,
        ];
        let triple =
            Pattern::compile(quoted, TRIPLE_RELATION, identity, &mut variables, store).await?;
        let predicate = store
            .resolve_iri(predicate.as_str())
            .await
            .map_err(ReasoningError::Storage)?;
        let object = store
            .resolve_iri(object.as_str())
            .await
            .map_err(ReasoningError::Storage)?;
        Ok(Self {
            id,
            body,
            triple,
            predicate,
            object,
        })
    }
}

pub(super) enum Body<T> {
    One(Pattern<T>),
    Two([Pattern<T>; 2]),
}
impl<T> Body<T> {
    pub fn first(&self) -> &Pattern<T> {
        match self {
            Self::One(first) => first,
            Self::Two([first, _]) => first,
        }
    }
}

impl<T: Clone + Eq, const N: usize> Pattern<T, N> {
    pub fn probe(&self, bindings: &Bindings<T>) -> [Option<T>; N] {
        std::array::from_fn(|i| match &self.0[i] {
            Slot::Constant(value) => Some(value.clone()),
            Slot::Variable(index) => bindings[*index].clone(),
        })
    }

    pub fn bind(&self, tuple: &[T; N], mut bindings: Bindings<T>) -> Option<Bindings<T>> {
        for (slot, term) in self.0.iter().zip(tuple) {
            match slot {
                Slot::Constant(value) if value != term => return None,
                Slot::Variable(index) => match &bindings[*index] {
                    Some(value) if value != term => return None,
                    None => bindings[*index] = Some(term.clone()),
                    Some(_) => {}
                },
                Slot::Constant(_) => {}
            }
        }
        Some(bindings)
    }
}
impl<T: Clone + Eq> Pattern<T> {
    pub fn ground(&self, bindings: &Bindings<T>) -> Option<[T; 3]> {
        let [s, p, o] = self.probe(bindings);
        match (s, p, o) {
            (Some(s), Some(p), Some(o)) => Some([s, p, o]),
            _ => None,
        }
    }
}

impl<T: Clone + Eq + Send + Sync> Clause<T> {
    pub async fn compile<S: ReasoningStore<Term = T>>(
        id: RuleId,
        store: &mut S,
    ) -> Result<Self, ReasoningError<S::Error>> {
        let rule = id.definition();
        // bounded: two three-term body atoms bind at most six variables.
        let mut variables = Vec::with_capacity(VARIABLES);
        let body = match rule.body.as_slice() {
            [Literal::Positive(first)] => Body::One(
                Pattern::compile(
                    first,
                    TRIPLE_RELATION,
                    DefinitionId::Rdf(id),
                    &mut variables,
                    store,
                )
                .await?,
            ),
            [Literal::Positive(first), Literal::Positive(second)] => Body::Two([
                Pattern::compile(
                    first,
                    TRIPLE_RELATION,
                    DefinitionId::Rdf(id),
                    &mut variables,
                    store,
                )
                .await?,
                Pattern::compile(
                    second,
                    TRIPLE_RELATION,
                    DefinitionId::Rdf(id),
                    &mut variables,
                    store,
                )
                .await?,
            ]),
            _ => return Err(ReasoningError::UnsupportedRule(id)),
        };
        let head = Pattern::compile(
            &rule.head,
            TRIPLE_RELATION,
            DefinitionId::Rdf(id),
            &mut variables,
            store,
        )
        .await?;
        Ok(Self { id, head, body })
    }
}
impl<T: Clone + Eq + Send + Sync, const N: usize> Pattern<T, N> {
    async fn compile<S: ReasoningStore<Term = T>>(
        atom: &Atom,
        relation: &str,
        id: DefinitionId,
        variables: &mut Vec<VariableSymbol>,
        store: &mut S,
    ) -> Result<Self, ReasoningError<S::Error>> {
        if atom.predicate.as_str() != relation || atom.terms.len() != N {
            return Err(id.error());
        }
        let mut result = std::array::from_fn(|_| Slot::Variable(0));
        for (index, term) in atom.terms.iter().enumerate() {
            result[index] = match term {
                Term::Constant(iri) => Slot::Constant(
                    store
                        .resolve_iri(iri.as_str())
                        .await
                        .map_err(ReasoningError::Storage)?,
                ),
                Term::Variable(variable) => {
                    let slot = if let Some(slot) = variables.iter().position(|v| v == variable) {
                        slot
                    } else {
                        if variables.len() == VARIABLES {
                            return Err(id.error());
                        }
                        variables.push(variable.clone());
                        variables.len() - 1
                    };
                    Slot::Variable(slot)
                }
                _ => return Err(id.error()),
            };
        }
        Ok(Pattern(result))
    }
}

pub(super) struct LiteralClause<T> {
    pub id: Rdf12LiteralRuleId,
    pub head: Pattern<T>,
    pub body: Pattern<T, 2>,
    pub datatype: T,
}
impl<T: Clone + Eq + Send + Sync> LiteralClause<T> {
    pub async fn compile<S: ReasoningStore<Term = T>>(
        id: Rdf12LiteralRuleId,
        store: &mut S,
    ) -> Result<Self, ReasoningError<S::Error>> {
        let rule = id.definition();
        let [Literal::Positive(body)] = rule.body.as_slice() else {
            return Err(ReasoningError::UnsupportedLiteralRule(id));
        };
        // bounded: one binary metadata premise binds at most two variables.
        let mut variables = Vec::with_capacity(2);
        let identity = DefinitionId::Literal(id);
        let body = Pattern::compile(
            body,
            RDF12_LITERAL_DATATYPE_RELATION,
            identity,
            &mut variables,
            store,
        )
        .await?;
        let head =
            Pattern::compile(&rule.head, TRIPLE_RELATION, identity, &mut variables, store).await?;
        let datatype = store
            .resolve_iri(id.datatype().iri())
            .await
            .map_err(ReasoningError::Storage)?;
        Ok(Self {
            id,
            head,
            body,
            datatype,
        })
    }
}

pub(super) struct ConflictClause<T> {
    pub id: Rdf12DatatypeConflictId,
    pub body: [Pattern<T>; 2],
    pub head: Pattern<T, 1>,
}
impl<T: Clone + Eq + Send + Sync> ConflictClause<T> {
    pub async fn compile<S: ReasoningStore<Term = T>>(
        id: Rdf12DatatypeConflictId,
        store: &mut S,
    ) -> Result<Self, ReasoningError<S::Error>> {
        let rule = id.definition();
        let identity = DefinitionId::Conflict(id);
        let [Literal::Positive(first), Literal::Positive(second)] = rule.body.as_slice() else {
            return Err(identity.error());
        };
        // bounded: two fixed triple premises and one unary diagnostic head.
        let mut variables = Vec::with_capacity(VARIABLES);
        let body = [
            Pattern::compile(first, TRIPLE_RELATION, identity, &mut variables, store).await?,
            Pattern::compile(second, TRIPLE_RELATION, identity, &mut variables, store).await?,
        ];
        let head = Pattern::compile(
            &rule.head,
            id.witness_relation(),
            identity,
            &mut variables,
            store,
        )
        .await?;
        Ok(Self { id, body, head })
    }
}
