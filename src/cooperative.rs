//! Runtime-independent cooperative checkpoints for cached-ready execution loops.
use std::{future::poll_fn, task::Poll};

pub(crate) const QUANTUM: u64 = 128;

pub(crate) async fn checkpoint(steps: u64) {
    if !steps.is_multiple_of(QUANTUM) {
        return;
    }
    let mut yielded = false;
    poll_fn(move |context| {
        if yielded {
            return Poll::Ready(());
        }
        yielded = true;
        context.waker().wake_by_ref();
        Poll::Pending
    })
    .await;
}
