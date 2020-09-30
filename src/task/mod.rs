use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.0 == true {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// Yield the current task
///
/// Schedules the current task to be resumed again and then yields to any
/// other ready tasks.
///
/// This can be used in long-running loops to not block all other tasks from running.
pub async fn yield_now() {
    YieldNow(false).await;
}
