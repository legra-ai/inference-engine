//! Exact shared numeric schedules and explicit layer lifecycle context.

use inference_engine::shacl12::{Controller, Limits, Plan, PlanError, Rule, RuleBackend, Work};
use rdf_structural_model::Literal;
use shacl_structural_model::inference::{RuleNumber, RuleSchedule};
use std::{convert::Infallible, num::NonZeroU64};

fn number(value: &str) -> RuleNumber {
    let datatype = "http://www.w3.org/2001/XMLSchema#decimal".parse().unwrap();
    let literal: Literal = Literal::typed(value, datatype).unwrap();
    serde_json::from_slice(&serde_json::to_vec(&literal).unwrap()).unwrap()
}
#[derive(Default)]
struct Backend {
    calls: Vec<u8>,
    layers: Vec<Vec<u8>>,
    finished: Vec<Vec<u8>>,
    snapshots: usize,
}
impl RuleBackend for Backend {
    type Rule = u8;
    type Snapshot = ();
    type Error = Infallible;
    async fn snapshot(&mut self) -> Result<(), Infallible> {
        self.snapshots += 1;
        Ok(())
    }
    async fn begin_layer(&mut self, rules: &[Rule<u8>], _: &mut Work) -> Result<(), Infallible> {
        self.layers
            .push(rules.iter().map(|rule| *rule.id()).collect());
        Ok(())
    }
    async fn execute(&mut self, id: &u8, _: &(), _: u64, _: &mut Work) -> Result<(), Infallible> {
        self.calls.push(*id);
        Ok(())
    }
    async fn merge_phase(&mut self, _: &mut Work) -> Result<u64, Infallible> {
        Ok(0)
    }
    async fn finish_layer(&mut self, rules: &[Rule<u8>], _: &mut Work) -> Result<(), Infallible> {
        self.finished
            .push(rules.iter().map(|rule| *rule.id()).collect());
        Ok(())
    }
    async fn finish(&mut self, _: &mut Work) -> Result<(), Infallible> {
        Ok(())
    }
    fn is_cancelled(&self) -> bool {
        false
    }
}
fn controller() -> Controller {
    Controller::new(Limits::new(
        8,
        NonZeroU64::new(1000).unwrap(),
        NonZeroU64::new(10).unwrap(),
    ))
}
#[tokio::test]
async fn equal_explicit_zero_and_absent_order_share_one_phase_without_float_rounding() {
    // bounded: five finite declarations, testing absent and lexical numeric equality.
    let plan = Plan::try_new(
        vec![
            Rule::new(
                1,
                RuleSchedule::new().with_order(number("9007199254740993")),
            ),
            Rule::new(
                2,
                RuleSchedule::new().with_order(number("9007199254740992")),
            ),
            Rule::new(3, RuleSchedule::new().with_order(number("+000.000"))),
            Rule::new(4, RuleSchedule::new()),
            Rule::new(5, RuleSchedule::new().with_order(number("-0.0"))),
        ],
        5,
    )
    .unwrap();
    let mut backend = Backend::default();
    controller().run(&plan, &mut backend).await.unwrap();
    assert_eq!(backend.calls, [3, 4, 5, 2, 1]);
    assert_eq!(backend.snapshots, 3);
    assert_eq!(backend.layers, [vec![3, 4, 5, 2, 1]]);
    assert_eq!(backend.finished, backend.layers);
}
#[tokio::test]
async fn layer_then_run_once_then_numeric_order_control_execution() {
    // bounded: four independent checked rules across three layers.
    let plan = Plan::try_new(
        vec![
            Rule::new(
                4,
                RuleSchedule::new().with_layer(number("9007199254740993")),
            ),
            Rule::new(
                2,
                RuleSchedule::new()
                    .with_layer(number("-1"))
                    .with_order(number("-100")),
            ),
            Rule::new(
                1,
                RuleSchedule::new()
                    .with_layer(number("-1.0"))
                    .with_order(number("100"))
                    .with_run_once(true),
            ),
            Rule::new(3, RuleSchedule::new()),
        ],
        4,
    )
    .unwrap();
    let mut backend = Backend::default();
    let report = controller().run(&plan, &mut backend).await.unwrap();
    assert_eq!(backend.calls, [1, 2, 3, 4]);
    assert_eq!(backend.layers, [vec![1, 2], vec![3], vec![4]]);
    assert_eq!(backend.finished, backend.layers);
    assert_eq!(report.iterations, 3);
}
#[test]
fn duplicate_handles_and_excess_rules_fail_before_any_execution() {
    assert_eq!(
        Plan::try_new(
            vec![
                Rule::new(1, RuleSchedule::new()),
                Rule::new(1, RuleSchedule::new())
            ],
            2
        )
        .unwrap_err(),
        PlanError::DuplicateRule
    );
    assert_eq!(
        Plan::try_new(vec![Rule::new(1, RuleSchedule::new())], 0).unwrap_err(),
        PlanError::TooManyRules
    );
}
