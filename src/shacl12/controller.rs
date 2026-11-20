use super::{
    ExecutionError, Limit, Limits, Plan, Report, Rule, RuleBackend, Work, plan::number_cmp,
};
use std::cmp::Ordering;

/// Native scheduling controller for the pinned SHACL 1.2 execution contract.
pub struct Controller {
    limits: Limits,
}
impl Controller {
    /// Construct a controller with explicit finite attempt bounds.
    pub const fn new(limits: Limits) -> Self {
        Self { limits }
    }

    /// Execute each layer's run-once phase, then iterate to a fixed point.
    /// Same-order rules share one immutable snapshot and merge only afterward.
    ///
    /// # Errors
    /// Any backend failure, cancellation or exhausted limit invalidates the
    /// entire attempt, including all earlier staged phases. Never publish on error.
    pub async fn run<B: RuleBackend>(
        &self,
        plan: &Plan<B::Rule>,
        backend: &mut B,
    ) -> Result<Report, ExecutionError<B::Error>> {
        let mut report = Report::default();
        let mut work = Work::new(self.limits.work);
        let mut start = 0;
        self.check(backend, &work)?;
        while start < plan.rules.len() {
            let end = start
                + plan.rules[start..].partition_point(|rule| {
                    number_cmp(rule.schedule.layer(), plan.rules[start].schedule.layer())
                        == Ordering::Equal
                });
            work.charge()
                .map_err(|_| ExecutionError::Limit(Limit::Work))?;
            backend
                .begin_layer(&mut work)
                .await
                .map_err(ExecutionError::Backend)?;
            self.check(backend, &work)?;
            let rules = &plan.rules[start..end];
            let once = rules.partition_point(|rule| rule.schedule.run_once() == Some(true));
            self.iteration(&rules[..once], backend, &mut work, &mut report)
                .await?;
            let iterating = &rules[once..];
            if !iterating.is_empty() {
                let mut iteration = 0;
                loop {
                    if iteration == self.limits.iterations {
                        return Err(ExecutionError::Limit(Limit::Iterations));
                    }
                    iteration += 1;
                    report.iterations += 1;
                    if !self
                        .iteration(iterating, backend, &mut work, &mut report)
                        .await?
                    {
                        break;
                    }
                }
            }
            backend
                .finish_layer(&mut work)
                .await
                .map_err(ExecutionError::Backend)?;
            self.check(backend, &work)?;
            start = end;
        }
        backend
            .finish(&mut work)
            .await
            .map_err(ExecutionError::Backend)?;
        self.check(backend, &work)?;
        report.work = work.used();
        Ok(report)
    }

    async fn iteration<B: RuleBackend>(
        &self,
        rules: &[Rule<B::Rule>],
        backend: &mut B,
        work: &mut Work,
        report: &mut Report,
    ) -> Result<bool, ExecutionError<B::Error>> {
        let mut start = 0;
        let mut changed = false;
        while start < rules.len() {
            self.check(backend, work)?;
            work.charge()
                .map_err(|_| ExecutionError::Limit(Limit::Work))?;
            let end = start
                + rules[start..].partition_point(|rule| {
                    number_cmp(rule.schedule.order(), rules[start].schedule.order())
                        == Ordering::Equal
                });
            let snapshot = backend.snapshot().await.map_err(ExecutionError::Backend)?;
            self.check(backend, work)?;
            report.phases += 1;
            for rule in &rules[start..end] {
                work.charge()
                    .map_err(|_| ExecutionError::Limit(Limit::Work))?;
                backend
                    .execute(&rule.id, &snapshot, report.phases, work)
                    .await
                    .map_err(ExecutionError::Backend)?;
                self.check(backend, work)?;
                crate::cooperative::checkpoint(work.used()).await;
            }
            let added = backend
                .merge_phase(work)
                .await
                .map_err(ExecutionError::Backend)?;
            self.check(backend, work)?;
            report.inferred = report
                .inferred
                .checked_add(added)
                .filter(|total| *total <= self.limits.inferred.get())
                .ok_or(ExecutionError::Limit(Limit::Inferred))?;
            changed |= added != 0;
            start = end;
        }
        Ok(changed)
    }
    fn check<B: RuleBackend>(
        &self,
        backend: &B,
        work: &Work,
    ) -> Result<(), ExecutionError<B::Error>> {
        if backend.is_cancelled() {
            return Err(ExecutionError::Cancelled);
        }
        if work.exceeded() {
            return Err(ExecutionError::Limit(Limit::Work));
        }
        Ok(())
    }
}
