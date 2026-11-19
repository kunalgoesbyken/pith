use std::time::Duration;

use async_channel::Receiver;
use async_io::Timer;
use futures_lite::future::or;

pub(crate) const DEADLINE: Duration = Duration::from_millis(500);
pub(crate) const STARTUP_DEADLINE: Duration = Duration::from_secs(5);

pub(crate) enum Bounded<T> {
    Done(T),
    TimedOut,
    Stopped,
}

pub(crate) async fn stop_requested(stopped: &Receiver<()>) {
    let _ = stopped.recv().await;
}

pub(crate) async fn within<T>(limit: Duration, work: impl Future<Output = T>) -> Option<T> {
    or(async { Some(work.await) }, async {
        Timer::after(limit).await;
        None
    })
    .await
}

pub(crate) async fn bounded<T>(
    stopped: &Receiver<()>,
    work: impl Future<Output = T>,
) -> Bounded<T> {
    or(
        async {
            match within(DEADLINE, work).await {
                Some(done) => Bounded::Done(done),
                None => Bounded::TimedOut,
            }
        },
        async {
            stop_requested(stopped).await;
            Bounded::Stopped
        },
    )
    .await
}
