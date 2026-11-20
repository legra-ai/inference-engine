//! Fail-fast and cooperative execution boundaries of the native controller.

use inference_engine::shacl12::{
    Controller, ExecutionError, Limit, Limits, Plan, Rule, RuleBackend, Work,
};
use shacl_structural_model::inference::RuleSchedule;
use std::{
    io,
    num::NonZeroU64,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Default)]
struct Backend {
    cancelled: Arc<AtomicBool>,
    charges: u64,
    fail: bool,
    additions: u64,
    merged: usize,
    finished: bool,
}
impl RuleBackend for Backend {
    type Rule = u16;
    type Snapshot = ();
    type Error = io::Error;
    async fn snapshot(&mut self) -> Result<(), io::Error> {
        Ok(())
    }
    async fn begin_layer(&mut self, _: &[Rule<Self::Rule>], _: &mut Work) -> Result<(), io::Error> {
        Ok(())
    }
    async fn execute(&mut self, _: &u16, _: &(), _: u64, work: &mut Work) -> Result<(), io::Error> {
        for _ in 0..self.charges {
            let _ = work.charge();
        }
        if self.fail {
            return Err(io::Error::other("native query failed"));
        }
        Ok(())
    }
    async fn merge_phase(&mut self, _: &mut Work) -> Result<u64, io::Error> {
        self.merged += 1;
        Ok(self.additions)
    }
    async fn finish_layer(
        &mut self,
        _: &[Rule<Self::Rule>],
        _: &mut Work,
    ) -> Result<(), io::Error> {
        Ok(())
    }
    async fn finish(&mut self, _: &mut Work) -> Result<(), io::Error> {
        self.finished = true;
        Ok(())
    }
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}
fn plan(count: u16) -> Plan<u16> {
    // bounded: each test specifies at most 1,000 finite rule handles.
    Plan::try_new(
        (0..count)
            .map(|id| Rule::new(id, RuleSchedule::new()))
            .collect(),
        1000,
    )
    .unwrap()
}
fn controller(iterations: u32, work: u64, output: u64) -> Controller {
    Controller::new(Limits::new(
        iterations,
        NonZeroU64::new(work).unwrap(),
        NonZeroU64::new(output).unwrap(),
    ))
}

#[tokio::test]
async fn ignored_backend_work_exhaustion_still_invalidates_the_attempt() {
    let mut backend = Backend {
        charges: 100,
        ..Backend::default()
    };
    assert!(matches!(
        controller(8, 10, 10).run(&plan(1), &mut backend).await,
        Err(ExecutionError::Limit(Limit::Work))
    ));
    assert_eq!(backend.merged, 0);
    assert!(!backend.finished);
}
#[tokio::test]
async fn native_failure_is_not_an_empty_successful_phase() {
    let mut backend = Backend {
        fail: true,
        ..Backend::default()
    };
    assert!(matches!(
        controller(8, 100, 10).run(&plan(1), &mut backend).await,
        Err(ExecutionError::Backend(_))
    ));
    assert_eq!(backend.merged, 0);
    assert!(!backend.finished);
}
#[tokio::test]
async fn output_and_iteration_limits_never_return_a_completion_report() {
    let mut backend = Backend {
        additions: 1,
        ..Backend::default()
    };
    assert!(matches!(
        controller(1, 100, 10).run(&plan(1), &mut backend).await,
        Err(ExecutionError::Limit(Limit::Iterations))
    ));
    assert!(!backend.finished);
    assert!(matches!(
        controller(8, 100, 1).run(&plan(1), &mut backend).await,
        Err(ExecutionError::Limit(Limit::Inferred))
    ));
    assert!(!backend.finished);
}
#[tokio::test]
async fn cancelled_attempt_does_not_start_or_finalize() {
    let mut backend = Backend::default();
    backend.cancelled.store(true, Ordering::Relaxed);
    assert!(matches!(
        controller(8, 100, 10).run(&plan(1), &mut backend).await,
        Err(ExecutionError::Cancelled)
    ));
    assert_eq!(backend.merged, 0);
    assert!(!backend.finished);
}
#[tokio::test]
async fn cached_ready_backend_cannot_starve_a_cancellation_task() {
    let mut backend = Backend {
        charges: 127,
        ..Backend::default()
    };
    let flag = backend.cancelled.clone();
    let cancellation = tokio::spawn(async move {
        flag.store(true, Ordering::Relaxed);
    });
    let result = controller(8, 1_000_000, 10)
        .run(&plan(1000), &mut backend)
        .await;
    cancellation.await.unwrap();
    assert!(matches!(result, Err(ExecutionError::Cancelled)));
    assert!(!backend.finished);
}
