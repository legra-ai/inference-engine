//! Backend stores need not be Sync to execute on a multithreaded runtime.

use inference_engine::reasoning::{Reasoner, ReasoningStore};
use std::future::Future;

fn require_send(_: impl Future + Send) {}

#[allow(dead_code)]
fn executor_future_is_send<S: ReasoningStore>(reasoner: &Reasoner, store: &mut S) {
    require_send(reasoner.run(store));
}
