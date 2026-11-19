use std::time::Duration;

use async_channel::Receiver;
use async_io::Timer;
use futures_lite::future::or;

pub(crate) const DEADLINE: Duration = Duration::from_millis(500);

pub(crate) enum Bounded<T> {
    Done(T),
    TimedOut,
    Stopped,
}

pub(crate) async fn within<T>(work: impl Future<Output = T>) -> Option<T> {
    or(async { Some(work.await) }, async {
        Timer::after(DEADLINE).await;
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
            match within(work).await {
                Some(done) => Bounded::Done(done),
                None => Bounded::TimedOut,
            }
        },
        async {
            let _ = stopped.recv().await;
            Bounded::Stopped
        },
    )
    .await
}
