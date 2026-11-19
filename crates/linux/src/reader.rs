use std::pin::pin;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

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
use crate::bounded::{Bounded, STARTUP_DEADLINE, bounded, stop_requested, within};
use crate::error::ReaderError;
use crate::field::read;
use crate::focus::{Effect, Focus, Tracker};
use crate::reading::Reading;

const READ_ATTEMPTS: usize = 3;
const SPAWN_TIMEOUT: Duration = Duration::from_secs(15);

pub type ReadingCallback = Box<dyn Fn(Result<Reading, ReaderError>) + Send>;

pub struct Reader {
    stop: Sender<()>,
    wake: Sender<()>,
    config: Arc<Mutex<Config>>,
    thread: Option<JoinHandle<()>>,
}

struct Control {
    config: Arc<Mutex<Config>>,
    stopped: Receiver<()>,
    woken: Receiver<()>,
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
    Reconfigured,
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
        let (wake, woken) = unbounded();
        let (ready, started) = mpsc::channel();
        let config = Arc::new(Mutex::new(config));
        let control = Control {
            config: config.clone(),
            stopped,
            woken,
        };
        let thread = std::thread::Builder::new()
            .name("pith-atspi".into())
            .spawn(move || serve(control, consent, ready, on_reading))
            .map_err(ReaderError::Spawn)?;
        match started.recv_timeout(SPAWN_TIMEOUT) {
            Ok(Ok(())) => Ok(Self {
                stop,
                wake,
                config,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(RecvTimeoutError::Timeout) => {
                stop.close();
                Err(ReaderError::StartupTimedOut)
            }
            Err(RecvTimeoutError::Disconnected) => Err(ReaderError::Stopped),
        }
    }

    pub fn set_config(&self, config: &Config) {
        *self.config.lock().unwrap_or_else(PoisonError::into_inner) = config.clone();
        let _ = self.wake.try_send(());
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

async fn connect() -> Result<AccessibilityConnection, ReaderError> {
    let connection = AccessibilityConnection::new().await?;
    connection.register_event::<StateChangedEvent>().await?;
    connection.register_event::<TextCaretMovedEvent>().await?;
    connection.register_event::<TextChangedEvent>().await?;
    connection
        .register_event::<TextSelectionChangedEvent>()
        .await?;
    Ok(connection)
}

impl Session {
    async fn open(consent: Option<Consent>) -> Result<Self, ReaderError> {
        let lease = Lease::acquire(consent).await?;
        match within(STARTUP_DEADLINE, connect()).await {
            Some(Ok(connection)) => Ok(Self { connection, lease }),
            Some(Err(error)) => {
                lease.release().await;
                Err(error)
            }
            None => {
                lease.release().await;
                Err(ReaderError::StartupTimedOut)
            }
        }
    }

    async fn run(&self, control: &Control, on_reading: &ReadingCallback) -> Exit {
        let bus = self.connection.connection();
        let mut events = pin!(self.connection.event_stream().filter_map(Result::ok));
        let mut tracker = Tracker::default();
        let mut last: Option<Reading> = None;
        loop {
            let mut changed = false;
            let mut batch = Vec::new();
            match next(&mut events, control).await {
                Next::Event(event) => batch.push(*event),
                Next::Reconfigured => {
                    while control.woken.try_recv().is_ok() {}
                    changed = true;
                }
                Next::Stopped => return Exit::Stopped,
                Next::Closed => return Exit::BusClosed,
            }
            while let Some(pending) = poll_once(events.next()).await {
                match pending {
                    Some(event) => batch.push(event),
                    None => return Exit::BusClosed,
                }
            }
            for event in batch {
                match tracker.apply(bus, &control.stopped, event).await {
                    Effect::Changed => changed = true,
                    Effect::Unchanged => {}
                    Effect::Stopped => return Exit::Stopped,
                }
            }
            if !changed {
                continue;
            }
            let config = control
                .config
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            let reading = match tracker.focus_mut() {
                Some(focus) if config.enabled => {
                    match snapshot(bus, &control.stopped, focus, &config).await {
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

async fn next(events: &mut (impl Stream<Item = Event> + Unpin), control: &Control) -> Next {
    or(
        async {
            match events.next().await {
                Some(event) => Next::Event(Box::new(event)),
                None => Next::Closed,
            }
        },
        or(
            async {
                stop_requested(&control.stopped).await;
                Next::Stopped
            },
            async {
                let _ = control.woken.recv().await;
                Next::Reconfigured
            },
        ),
    )
    .await
}

fn serve(
    control: Control,
    consent: Option<Consent>,
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
    if let Exit::BusClosed = block_on(session.run(&control, &on_reading)) {
        on_reading(Ok(Reading::Nothing));
        on_reading(Err(ReaderError::BusClosed));
    }
    block_on(session.lease.release());
}
