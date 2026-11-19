use std::pin::pin;
use std::sync::mpsc;
use std::thread::JoinHandle;

use async_channel::{Receiver, Sender, unbounded};
use atspi::AccessibilityConnection;
use atspi::events::Event;
use atspi::events::object::{
    StateChangedEvent, TextCaretMovedEvent, TextChangedEvent, TextSelectionChangedEvent,
};
use futures_lite::future::{block_on, or, poll_once};
use futures_lite::{Stream, StreamExt};
use pith_core::{Config, log};
use zbus::Connection;

use crate::accessibility::{Consent, Lease};
use crate::bounded::{Bounded, bounded};
use crate::error::ReaderError;
use crate::field::read;
use crate::focus::{Effect, Focus, Tracker};
use crate::reading::Reading;

const READ_ATTEMPTS: usize = 3;

pub type ReadingCallback = Box<dyn Fn(Result<Reading, ReaderError>) + Send>;

pub struct Reader {
    stop: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

struct Session {
    connection: AccessibilityConnection,
    lease: Lease,
}

enum Exit {
    Stopped,
    BusClosed,
}

enum Next {
    Event(Box<Event>),
    Stopped,
    Closed,
}

impl Reader {
    pub fn spawn(
        config: Config,
        consent: Option<Consent>,
        on_reading: ReadingCallback,
    ) -> Result<Self, ReaderError> {
        let (stop, stopped) = unbounded();
        let (ready, started) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("pith-atspi".into())
            .spawn(move || serve(config, consent, stopped, ready, on_reading))
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
    async fn open(consent: Option<Consent>) -> Result<Self, ReaderError> {
        let lease = Lease::acquire(consent).await?;
        let connection = AccessibilityConnection::new().await?;
        connection.register_event::<StateChangedEvent>().await?;
        connection.register_event::<TextCaretMovedEvent>().await?;
        connection.register_event::<TextChangedEvent>().await?;
        connection
            .register_event::<TextSelectionChangedEvent>()
            .await?;
        Ok(Self { connection, lease })
    }

    async fn run(
        &self,
        config: &Config,
        stopped: &Receiver<()>,
        on_reading: &ReadingCallback,
    ) -> Exit {
        let bus = self.connection.connection();
        let mut events = pin!(self.connection.event_stream().filter_map(Result::ok));
        let mut tracker = Tracker::default();
        let mut last: Option<Reading> = None;
        loop {
            let mut batch = match next(&mut events, stopped).await {
                Next::Event(event) => vec![*event],
                Next::Stopped => return Exit::Stopped,
                Next::Closed => return Exit::BusClosed,
            };
            while let Some(pending) = poll_once(events.next()).await {
                match pending {
                    Some(event) => batch.push(event),
                    None => return Exit::BusClosed,
                }
            }
            let mut changed = false;
            for event in batch {
                match tracker.apply(bus, stopped, event).await {
                    Effect::Changed => changed = true,
                    Effect::Unchanged => {}
                    Effect::Stopped => return Exit::Stopped,
                }
            }
            if !changed {
                continue;
            }
            let reading = match tracker.focus_mut() {
                Some(focus) if config.enabled => {
                    match snapshot(bus, stopped, focus, config).await {
                        Some(reading) => reading,
                        None => return Exit::Stopped,
                    }
                }
                _ => Reading::Nothing,
            };
            if last.as_ref() != Some(&reading) {
                last = Some(reading.clone());
                on_reading(Ok(reading));
            }
        }
    }

    async fn release(self) {
        self.lease.release().await;
    }
}

async fn snapshot(
    bus: &Connection,
    stopped: &Receiver<()>,
    focus: &mut Focus,
    config: &Config,
) -> Option<Reading> {
    for _ in 0..READ_ATTEMPTS {
        match bounded(stopped, read(bus, focus, config)).await {
            Bounded::Done(Ok(reading)) => return Some(reading),
            Bounded::Done(Err(error)) => log(&format!("atspi read failed: {error}")),
            Bounded::TimedOut => log("atspi read timed out"),
            Bounded::Stopped => return None,
        }
    }
    Some(Reading::Nothing)
}

async fn next(events: &mut (impl Stream<Item = Event> + Unpin), stopped: &Receiver<()>) -> Next {
    or(
        async {
            match events.next().await {
                Some(event) => Next::Event(Box::new(event)),
                None => Next::Closed,
            }
        },
        async {
            let _ = stopped.recv().await;
            Next::Stopped
        },
    )
    .await
}

fn serve(
    config: Config,
    consent: Option<Consent>,
    stopped: Receiver<()>,
    ready: mpsc::Sender<Result<(), ReaderError>>,
    on_reading: ReadingCallback,
) {
    let session = match block_on(Session::open(consent)) {
        Ok(session) => session,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    if let Exit::BusClosed = block_on(session.run(&config, &stopped, &on_reading)) {
        on_reading(Ok(Reading::Nothing));
        on_reading(Err(ReaderError::BusClosed));
    }
    block_on(session.release());
}
