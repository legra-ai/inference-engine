//! Native controller regressions; query fixtures exercise a concrete backend separately.

use inference_engine::shacl12::{Controller, Limits, Plan, Rule, RuleBackend, Work};
use shacl_structural_model::inference::RuleSchedule;
use std::{collections::BTreeSet, convert::Infallible, num::NonZeroU64};

#[derive(Default)]
struct Backend {
    facts: BTreeSet<u8>,
    pending: BTreeSet<u8>,
    snapshots: Vec<BTreeSet<u8>>,
}

impl RuleBackend for Backend {
    type Rule = u8;
    type Snapshot = BTreeSet<u8>;
    type Error = Infallible;

    async fn snapshot(&mut self) -> Result<Self::Snapshot, Self::Error> {
        self.snapshots.push(self.facts.clone());
        Ok(self.facts.clone())
    }
    async fn begin_layer(&mut self, _: &mut Work) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn execute(
        &mut self,
        rule: &u8,
        snapshot: &Self::Snapshot,
        _: u64,
        _: &mut Work,
    ) -> Result<(), Self::Error> {
        if !snapshot.contains(&(3 - rule)) {
            self.pending.insert(*rule);
        }
        Ok(())
    }
    async fn merge_phase(&mut self, _: &mut Work) -> Result<u64, Self::Error> {
        let before = self.facts.len();
        self.facts.append(&mut self.pending);
        Ok((self.facts.len() - before) as u64)
    }
    async fn finish_layer(&mut self, _: &mut Work) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn finish(&mut self, _: &mut Work) -> Result<(), Self::Error> {
        Ok(())
    }
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[tokio::test]
async fn same_order_rules_do_not_observe_each_others_new_triples() {
    // bounded: two independently executing rules and at most two small facts.
    let plan = Plan::try_new(
        vec![
            Rule::new(1, RuleSchedule::new()),
            Rule::new(2, RuleSchedule::new()),
        ],
        2,
    )
    .unwrap();
    let mut backend = Backend::default();
    let report = Controller::new(Limits::new(
        8,
        NonZeroU64::new(100).unwrap(),
        NonZeroU64::new(10).unwrap(),
    ))
    .run(&plan, &mut backend)
    .await
    .unwrap();
    assert_eq!(backend.facts, BTreeSet::from([1, 2]));
    assert_eq!(report.inferred, 2);
    assert_eq!(report.iterations, 2);
    assert_eq!(
        backend.snapshots,
        vec![BTreeSet::new(), BTreeSet::from([1, 2])]
    );
}
