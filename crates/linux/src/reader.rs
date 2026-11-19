use std::pin::pin;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use async_channel::{Receiver, Sender, unbounded};
use async_io::Timer;
use atspi::AccessibilityConnection;
use atspi::events::Event;
use atspi::events::object::{StateChangedEvent, TextCaretMovedEvent, TextSelectionChangedEvent};
use futures_lite::future::{block_on, or, poll_once};
use futures_lite::{Stream, StreamExt};
use pith_core::{Config, log};
use zbus::Connection;

use crate::accessibility::{Consent, Lease};
use crate::bounded::{Bounded, STARTUP_DEADLINE, bounded, stop_requested, within};
use crate::error::{InsertError, Inserted, ReaderError, Target};
use crate::field::read;
use crate::focus::{Effect, Focus, Tracker};
use crate::held::Held;
use crate::insert::insert;
use crate::reading::Reading;
use crate::session::Quit;
use crate::settle::Settling;
use crate::x11::Desktop;

const READ_ATTEMPTS: usize = 3;
const SPAWN_TIMEOUT: Duration = Duration::from_secs(15);
const SPAWN_POLL: Duration = Duration::from_millis(100);

pub enum Report {
    Reading(Reading, Option<u64>),
    Inserted(Result<Inserted, InsertError>),
}

pub type ReportCallback = Box<dyn Fn(Result<Report, ReaderError>) + Send>;

enum Command {
    Read(u64),
    Insert(String, Target),
    Config(Box<Config>),
}

pub struct Reader {
    stop: Sender<()>,
    commands: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

struct Control {
    stopped: Receiver<()>,
    commands: Receiver<Command>,
}

struct Setup {
    config: Config,
    consent: Option<Consent>,
    desktop: Desktop,
    held: Held,
}

struct Session {
    connection: AccessibilityConnection,
    lease: Lease,
    config: Config,
    desktop: Desktop,
    held: Held,
}

enum Exit {
    Stopped,
    BusClosed,
}

enum Next {
    Event(Box<Event>),
    Command(Command),
    Settle,
    Stopped,
    Closed,
}

#[derive(Default)]
struct Pending {
    config: Option<Config>,
    changed: bool,
    report: bool,
    served: Option<u64>,
    inserts: Vec<(String, Target)>,
}

impl Pending {
    fn command(&mut self, command: Command) {
        match command {
            Command::Read(id) => {
                self.changed = true;
                self.served = Some(self.served.map_or(id, |served| served.max(id)));
            }
            Command::Insert(text, target) => self.inserts.push((text, target)),
            Command::Config(config) => self.config = Some(*config),
        }
    }
}

const NO_PRESS: u64 = 0;

impl Reader {
    pub fn spawn(
        config: Config,
        consent: Option<Consent>,
        desktop: Desktop,
        held: Held,
        quit: &Quit,
        on_report: ReportCallback,
    ) -> Result<Self, ReaderError> {
        let (stop, stopped) = unbounded();
        let (commands, received) = unbounded();
        let (ready, started) = mpsc::channel();
        let control = Control {
            stopped,
            commands: received,
        };
        let thread = std::thread::Builder::new()
            .name("pith-atspi".into())
            .spawn(move || {
                serve(
                    control,
                    Setup {
                        config,
                        consent,
                        desktop,
                        held,
                    },
                    ready,
                    on_report,
                )
            })
            .map_err(ReaderError::Spawn)?;
        let deadline = Instant::now() + SPAWN_TIMEOUT;
        loop {
            match started.recv_timeout(SPAWN_POLL) {
                Ok(Ok(())) => {
                    return Ok(Self {
                        stop,
                        commands,
                        thread: Some(thread),
                    });
                }
                Ok(Err(error)) => {
                    let _ = thread.join();
                    return Err(error);
                }
                Err(RecvTimeoutError::Disconnected) => return Err(ReaderError::Stopped),
                Err(RecvTimeoutError::Timeout) if quit.is_requested() => {
                    stop.close();
                    return Err(ReaderError::Stopped);
                }
                Err(RecvTimeoutError::Timeout) if Instant::now() >= deadline => {
                    stop.close();
                    return Err(ReaderError::StartupTimedOut);
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
    }

    pub fn request(&self, id: u64) {
        let _ = self.commands.try_send(Command::Read(id));
    }

    pub fn refresh(&self) {
        let _ = self.commands.try_send(Command::Read(NO_PRESS));
    }

    pub fn insert(&self, text: String, target: Target) {
        let _ = self.commands.try_send(Command::Insert(text, target));
    }

    pub fn set_config(&self, config: &Config) {
        let _ = self
            .commands
            .try_send(Command::Config(Box::new(config.clone())));
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
    connection
        .register_event::<TextSelectionChangedEvent>()
        .await?;
    Ok(connection)
}

impl Session {
    async fn open(setup: Setup) -> Result<Self, ReaderError> {
        let Setup {
            config,
            consent,
            desktop,
            held,
        } = setup;
        let lease = Lease::acquire(consent).await?;
        match within(STARTUP_DEADLINE, connect()).await {
            Some(Ok(connection)) => Ok(Self {
                connection,
                lease,
                config,
                desktop,
                held,
            }),
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

    async fn run(&self, control: &Control, on_report: &ReportCallback) -> Exit {
        let bus = self.connection.connection();
        let mut events = pin!(self.connection.event_stream().filter_map(Result::ok));
        let mut tracker = Tracker::default();
        let mut last: Option<Reading> = None;
        let mut settling = Settling::new();
        let mut config = self.config.clone();
        loop {
            let mut pending = Pending::default();
            let mut batch = Vec::new();
            match next(&mut events, control, settling.due).await {
                Next::Event(event) => batch.push(*event),
                Next::Command(command) => pending.command(command),
                Next::Settle => {
                    pending.changed = true;
                    pending.report = settling.take_forced();
                }
                Next::Stopped => return Exit::Stopped,
                Next::Closed => return Exit::BusClosed,
            }
            while let Some(pending_event) = poll_once(events.next()).await {
                match pending_event {
                    Some(event) => batch.push(event),
                    None => return Exit::BusClosed,
                }
            }
            while let Ok(command) = control.commands.try_recv() {
                pending.command(command);
            }
            if let Some(replacement) = pending.config.take() {
                config = replacement;
                pending.changed = true;
            }
            for event in batch {
                match tracker.apply(bus, &control.stopped, event).await {
                    Effect::Changed => pending.changed = true,
                    Effect::Unchanged => {}
                    Effect::Stopped => return Exit::Stopped,
                }
            }
            pending.changed |= tracker.loss_pending();
            for (text, target) in std::mem::take(&mut pending.inserts) {
                let outcome = match tracker.focus_mut() {
                    Some(focus) => {
                        match bounded(&control.stopped, insert(bus, focus, &text, &target)).await {
                            Bounded::Done(outcome) => outcome,
                            Bounded::TimedOut => Err(InsertError::TimedOut),
                            Bounded::Stopped => return Exit::Stopped,
                        }
                    }
                    None => Err(InsertError::NoField),
                };
                if matches!(
                    outcome,
                    Ok(Inserted::Unconfirmed) | Err(InsertError::TimedOut)
                ) {
                    settling.recheck();
                }
                on_report(Ok(Report::Inserted(outcome)));
                pending.changed = true;
                pending.report = true;
            }
            if !pending.changed {
                continue;
            }
            tracker.settle_loss(bus, self.held.is_held()).await;
            let works = self.desktop.work_areas();
            let reading = match tracker.focus_mut() {
                Some(focus) if config.enabled => {
                    match snapshot(bus, &control.stopped, focus, &config, &works).await {
                        Some(reading) => reading,
                        None => return Exit::Stopped,
                    }
                }
                _ => Reading::Nothing,
            };
            let changed = last.as_ref() != Some(&reading);
            settling.observe(&reading, changed, tracker.loss_pending());
            if changed || pending.report || pending.served.is_some() {
                last = Some(reading.clone());
                on_report(Ok(Report::Reading(reading, pending.served)));
            }
        }
    }
}

async fn snapshot(
    bus: &Connection,
    stopped: &Receiver<()>,
    focus: &mut Focus,
    config: &Config,
    works: &[pith_core::Rect],
) -> Option<Reading> {
    for _ in 0..READ_ATTEMPTS {
        match bounded(stopped, read(bus, focus, config, works)).await {
            Bounded::Done(Ok(reading)) => return Some(reading),
            Bounded::Done(Err(error)) => log(&format!("atspi read failed: {error}")),
            Bounded::TimedOut => log("atspi read timed out"),
            Bounded::Stopped => return None,
        }
    }
    Some(Reading::Nothing)
}

async fn next(
    events: &mut (impl Stream<Item = Event> + Unpin),
    control: &Control,
    settle_at: Option<Instant>,
) -> Next {
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
            or(
                async {
                    match control.commands.recv().await {
                        Ok(command) => Next::Command(command),
                        Err(_) => Next::Stopped,
                    }
                },
                async {
                    match settle_at {
                        Some(at) => {
                            Timer::at(at).await;
                        }
                        None => std::future::pending().await,
                    }
                    Next::Settle
                },
            ),
        ),
    )
    .await
}

fn serve(
    control: Control,
    setup: Setup,
    ready: mpsc::Sender<Result<(), ReaderError>>,
    on_report: ReportCallback,
) {
    let session = match block_on(Session::open(setup)) {
        Ok(session) => session,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    if let Exit::BusClosed = block_on(session.run(&control, &on_report)) {
        on_report(Ok(Report::Reading(Reading::Nothing, None)));
        on_report(Err(ReaderError::BusClosed));
    }
    block_on(session.lease.release());
}
