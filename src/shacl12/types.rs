use std::{error::Error, fmt, num::NonZeroU64};

/// Explicit attempt-wide controller limits. Backend-specific query/I/O limits
/// must also be enforced; these counters do not replace query cancellation.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub(super) iterations: u32,
    pub(super) work: NonZeroU64,
    pub(super) inferred: NonZeroU64,
}
impl Limits {
    /// Limit iterations per layer (including the final empty iteration), total
    /// charged work and total novel inferred triples, including temporary output.
    pub const fn new(iterations: u32, work: NonZeroU64, inferred: NonZeroU64) -> Self {
        Self {
            iterations,
            work,
            inferred,
        }
    }
}
/// Successfully completed counters; no report is returned after partial failure.
#[derive(Default, Debug, Eq, PartialEq)]
pub struct Report {
    /// Completed fixed-point iterations across layers, including empty rounds.
    pub iterations: u64,
    /// Executed same-order phases, including run-once phases.
    pub phases: u64,
    /// Completed rule invocations across all phases.
    pub executions: u64,
    /// Novel inferred triples before derived/temporary cleanup.
    pub inferred: u64,
    /// Total charged execution work, including backend reads and candidates.
    pub work: u64,
}
/// Attempt-wide charged work. An ignored exhaustion error remains sticky.
#[derive(Debug)]
pub struct Work {
    used: u64,
    maximum: u64,
    exceeded: bool,
}
impl Work {
    pub(super) const fn new(maximum: NonZeroU64) -> Self {
        Self {
            used: 0,
            maximum: maximum.get(),
            exceeded: false,
        }
    }
    /// Charge before performing a bounded unit of work.
    /// # Errors
    /// Exhaustion invalidates the entire attempt, even if a backend ignores it.
    pub fn charge(&mut self) -> Result<(), WorkExceeded> {
        if self.exceeded || self.used == self.maximum {
            self.exceeded = true;
            return Err(WorkExceeded);
        }
        self.used += 1;
        Ok(())
    }
    /// Successfully charged work units.
    pub const fn used(&self) -> u64 {
        self.used
    }
    pub(super) const fn exceeded(&self) -> bool {
        self.exceeded
    }
}
/// Work budget exhaustion, suitable for propagation through a backend error.
#[derive(Debug)]
pub struct WorkExceeded;
impl fmt::Display for WorkExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SHACL work limit exceeded")
    }
}
impl Error for WorkExceeded {}
/// Controller limit preventing successful completion.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Limit {
    /// A layer did not reach a fixed point within the allowed iterations.
    Iterations,
    /// Total charged work exhausted.
    Work,
    /// Novel inferred output exhausted.
    Inferred,
}
/// The entire attempt failed; discard all staged output.
#[derive(Debug)]
pub enum ExecutionError<E> {
    /// A native read, query or staging operation failed.
    Backend(E),
    /// Explicit execution limit exhausted.
    Limit(Limit),
    /// Attempt cancelled or superseded.
    Cancelled,
}
impl<E: fmt::Display> fmt::Display for ExecutionError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => write!(f, "SHACL backend failed: {error}"),
            Self::Limit(limit) => write!(f, "SHACL {limit:?} limit exceeded"),
            Self::Cancelled => f.write_str("SHACL attempt cancelled"),
        }
    }
}
impl<E: Error + 'static> Error for ExecutionError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        if let Self::Backend(error) = self {
            Some(error)
        } else {
            None
        }
    }
}
