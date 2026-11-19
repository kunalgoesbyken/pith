use std::pin::pin;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use async_channel::{Receiver, Sender, unbounded};
use async_io::Timer;
use atspi::events::Event;
use atspi::events::object::{
    StateChangedEvent, TextCaretMovedEvent, TextChangedEvent, TextSelectionChangedEvent,
};
use atspi::{AccessibilityConnection, ObjectRefOwned};
use futures_lite::future::{block_on, or, poll_once};
use futures_lite::{Stream, StreamExt};
use pith_core::{Config, FieldSnapshot};
use zbus::Connection;

use crate::accessibility::{Accessibility, Lease};
use crate::error::ReaderError;
use crate::field::read;
use crate::focus::Tracker;

const READ_TIMEOUT: Duration = Duration::from_millis(500);

pub type SnapshotCallback = Box<dyn Fn(Option<FieldSnapshot>) + Send>;

pub struct Reader {
    stop: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

struct Session {
    connection: AccessibilityConnection,
    _lease: Lease,
}

impl Reader {
    pub fn spawn(
        config: Config,
        accessibility: Accessibility,
        on_snapshot: SnapshotCallback,
    ) -> Result<Self, ReaderError> {
        let (stop, stopped) = unbounded();
        let (ready, started) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("pith-atspi".into())
            .spawn(move || serve(config, accessibility, stopped, ready, on_snapshot))
            .map_err(ReaderError::Spawn)?;
        match started.recv() {
            Ok(Ok(())) => Ok(Self {
                stop,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => Err(ReaderError::Stopped),
        }
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.stop.close();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Session {
    async fn open(accessibility: Accessibility) -> Result<Self, ReaderError> {
        let lease = accessibility.acquire().await?;
        let connection = AccessibilityConnection::new().await?;
        connection.register_event::<StateChangedEvent>().await?;
        connection.register_event::<TextCaretMovedEvent>().await?;
        connection.register_event::<TextChangedEvent>().await?;
        connection
            .register_event::<TextSelectionChangedEvent>()
            .await?;
        Ok(Self {
            connection,
            _lease: lease,
        })
    }

    async fn run(&self, config: &Config, stopped: &Receiver<()>, on_snapshot: &SnapshotCallback) {
        let bus = self.connection.connection();
        let mut events = pin!(self.connection.event_stream().filter_map(Result::ok));
        let mut tracker = Tracker::default();
        let mut last: Option<Option<FieldSnapshot>> = None;
        loop {
            let Some(first) = next_event(&mut events, stopped).await else {
                return;
            };
            let mut relevant = tracker.apply(bus, first).await;
            while let Some(pending) = poll_once(events.next()).await {
                let Some(event) = pending else {
                    return;
                };
                relevant |= tracker.apply(bus, event).await;
            }
            if !relevant {
                continue;
            }
            let snapshot = match tracker.focused() {
                Some(target) if config.enabled => read_within(bus, target, config).await,
                _ => None,
            };
            if last.as_ref() != Some(&snapshot) {
                last = Some(snapshot.clone());
                on_snapshot(snapshot);
            }
        }
    }
}

async fn next_event(
    events: &mut (impl Stream<Item = Event> + Unpin),
    stopped: &Receiver<()>,
) -> Option<Event> {
    or(events.next(), async {
        let _ = stopped.recv().await;
        None
    })
    .await
}

async fn read_within(
    bus: &Connection,
    target: &ObjectRefOwned,
    config: &Config,
) -> Option<FieldSnapshot> {
    or(
        async { read(bus, target, config).await.unwrap_or(None) },
        async {
            Timer::after(READ_TIMEOUT).await;
            None
        },
    )
    .await
}

fn serve(
    config: Config,
    accessibility: Accessibility,
    stopped: Receiver<()>,
    ready: mpsc::Sender<Result<(), ReaderError>>,
    on_snapshot: SnapshotCallback,
) {
    let session = match block_on(Session::open(accessibility)) {
        Ok(session) => session,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    block_on(session.run(&config, &stopped, &on_snapshot));
}
