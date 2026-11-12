use super::{ReasoningError, ReasoningStore};
use datalog_structural_model::{Atom, Literal, Term, VariableSymbol};
use rdf_reasoning::{RuleId, TRIPLE_RELATION};

const VARIABLES: usize = 6;
pub(super) type Bindings<T> = [Option<T>; VARIABLES];

pub(super) enum Slot<T> {
    Constant(T),
    Variable(usize),
}
pub(super) struct Pattern<T>([Slot<T>; 3]);
pub(super) struct Clause<T> {
    pub id: RuleId,
    pub head: Pattern<T>,
    pub body: [Pattern<T>; 2],
}

impl<T: Clone + Eq> Pattern<T> {
    pub fn probe(&self, bindings: &Bindings<T>) -> [Option<T>; 3] {
        std::array::from_fn(|i| match &self.0[i] {
            Slot::Constant(value) => Some(value.clone()),
            Slot::Variable(index) => bindings[*index].clone(),
        })
    }

    pub fn bind(&self, tuple: &[T; 3], mut bindings: Bindings<T>) -> Option<Bindings<T>> {
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

    pub fn ground<E>(
        &self,
        bindings: &Bindings<T>,
        id: RuleId,
    ) -> Result<[T; 3], ReasoningError<E>> {
        let [s, p, o] = self.probe(bindings);
        match (s, p, o) {
            (Some(s), Some(p), Some(o)) => Ok([s, p, o]),
            _ => Err(ReasoningError::UnsupportedRule(id)),
        }
    }
}

impl<T: Clone + Eq + Send + Sync> Clause<T> {
    pub async fn compile<S: ReasoningStore<Term = T>>(
        id: RuleId,
        store: &mut S,
    ) -> Result<Self, ReasoningError<S::Error>> {
        let rule = id.definition();
        let [Literal::Positive(first), Literal::Positive(second)] = rule.body.as_slice() else {
            return Err(ReasoningError::UnsupportedRule(id));
        };
        // bounded: two three-term body atoms bind at most six variables.
        let mut variables = Vec::with_capacity(VARIABLES);
        let first = Self::pattern(first, id, &mut variables, store).await?;
        let second = Self::pattern(second, id, &mut variables, store).await?;
        let head = Self::pattern(&rule.head, id, &mut variables, store).await?;
        Ok(Self {
            id,
            head,
            body: [first, second],
        })
    }

    async fn pattern<S: ReasoningStore<Term = T>>(
        atom: &Atom,
        id: RuleId,
        variables: &mut Vec<VariableSymbol>,
        store: &mut S,
    ) -> Result<Pattern<T>, ReasoningError<S::Error>> {
        if atom.predicate.as_str() != TRIPLE_RELATION || atom.terms.len() != 3 {
            return Err(ReasoningError::UnsupportedRule(id));
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
                            return Err(ReasoningError::UnsupportedRule(id));
                        }
                        variables.push(variable.clone());
                        variables.len() - 1
                    };
                    Slot::Variable(slot)
                }
                _ => return Err(ReasoningError::UnsupportedRule(id)),
            };
        }
        Ok(Pattern(result))
    }
}
