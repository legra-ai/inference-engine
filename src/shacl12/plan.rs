use shacl_structural_model::inference::{RuleNumber, RuleSchedule};
use std::{cmp::Ordering, error::Error, fmt};

/// One unique preflight-checked rule handle and its shared schedule declaration.
#[derive(Debug)]
pub struct Rule<Id> {
    pub(super) id: Id,
    pub(super) schedule: RuleSchedule,
}
impl<Id> Rule<Id> {
    /// Associate an existing backend rule with its checked declaration.
    pub const fn new(id: Id, schedule: RuleSchedule) -> Self {
        Self { id, schedule }
    }
    /// Original backend rule identity.
    pub const fn id(&self) -> &Id {
        &self.id
    }
    /// Original shared declaration, without lexical normalization.
    pub const fn schedule(&self) -> &RuleSchedule {
        &self.schedule
    }
}

/// Finite, explicitly capped rule programme sorted by exact layer/order values.
#[derive(Debug)]
pub struct Plan<Id> {
    pub(super) rules: Vec<Rule<Id>>,
}
impl<Id: Eq> Plan<Id> {
    /// Check the finite programme and sort it by layer, run-once phase and order.
    /// Inactive rules must have been removed by the preflight compiler. Repeated
    /// shape links must resolve to one rule handle before this boundary.
    ///
    /// # Errors
    /// Rejects excess rules or repeated identities; never silently truncates.
    pub fn try_new(mut rules: Vec<Rule<Id>>, maximum: usize) -> Result<Self, PlanError> {
        if rules.len() > maximum {
            return Err(PlanError::TooManyRules);
        }
        for (index, rule) in rules.iter().enumerate() {
            if rules[..index].iter().any(|earlier| earlier.id == rule.id) {
                return Err(PlanError::DuplicateRule);
            }
        }
        // bounded: the caller-specified rule-count cap was checked above.
        rules.sort_by(|a, b| {
            number_cmp(a.schedule.layer(), b.schedule.layer())
                .then_with(|| {
                    b.schedule
                        .run_once()
                        .unwrap_or(false)
                        .cmp(&a.schedule.run_once().unwrap_or(false))
                })
                .then_with(|| number_cmp(a.schedule.order(), b.schedule.order()))
        });
        Ok(Self { rules })
    }
}

/// A rejected rule programme, before graph execution starts.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PlanError {
    /// The explicit finite rule-count cap was exceeded.
    TooManyRules,
    /// One rule identity appeared more than once.
    DuplicateRule,
}
impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooManyRules => "SHACL rule-count limit exceeded",
            Self::DuplicateRule => "duplicate SHACL rule identity",
        })
    }
}
impl Error for PlanError {}

pub(super) fn number_cmp(a: Option<&RuleNumber>, b: Option<&RuleNumber>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.cmp_value(b),
        (None, None) => Ordering::Equal,
        (Some(a), None) => versus_zero(a),
        (None, Some(b)) => versus_zero(b).reverse(),
    }
}
fn versus_zero(number: &RuleNumber) -> Ordering {
    // Only apply the absent-property default. General numeric comparison belongs
    // to the checked shared model; no float or bounded integer conversion occurs.
    let lexical = number.literal().lexical();
    if lexical
        .bytes()
        .all(|byte| matches!(byte, b'0' | b'.' | b'+' | b'-'))
    {
        Ordering::Equal
    } else if lexical.starts_with('-') {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}
