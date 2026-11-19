mod effects;
mod inbox;
mod keyboard;
mod timers;

use std::fmt;
use std::io;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Instant;

use pith_core::{Action, Config, Engine, log};
use pith_infer::Worker;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use crate::accessibility::Consent;
use crate::error::ReaderError;
use crate::held::Held;
use crate::overlay::Overlay;
use crate::reader::Reader;
use crate::x11::{Clipboard, Desktop, Display, DisplayError, Grabs, listen};

use effects::Effects;
use inbox::Flow;
pub use inbox::{Inbound, Inbox};
use keyboard::Keyboard;
use timers::{Due, Timers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InsertPolicy {
    via_accessibility: bool,
    via_clipboard: bool,
}

impl Default for InsertPolicy {
    fn default() -> Self {
        Self {
            via_accessibility: true,
            via_clipboard: true,
        }
    }
}

impl InsertPolicy {
    pub fn accessibility_only() -> Self {
        Self {
            via_clipboard: false,
            ..Self::default()
        }
    }

    pub fn clipboard_only() -> Self {
        Self {
            via_accessibility: false,
            ..Self::default()
        }
    }

    fn falls_back(self) -> bool {
        self.via_accessibility && self.via_clipboard
    }
}

pub struct Options {
    pub insert: InsertPolicy,
    pub consent: Option<Consent>,
    pub observer: Option<Sender<Action>>,
}

#[derive(Debug)]
pub enum RunError {
    Display(DisplayError),
    Reader(ReaderError),
    Signals(io::Error),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Display(error) => write!(f, "{error}"),
            Self::Reader(error) => write!(f, "{error}"),
            Self::Signals(error) => write!(f, "signal handlers: {error}"),
        }
    }
}

impl std::error::Error for RunError {}

impl From<DisplayError> for RunError {
    fn from(error: DisplayError) -> Self {
        Self::Display(error)
    }
}

impl From<ReaderError> for RunError {
    fn from(error: ReaderError) -> Self {
        Self::Reader(error)
    }
}

impl From<io::Error> for RunError {
    fn from(error: io::Error) -> Self {
        Self::Signals(error)
    }
}

struct App {
    engine: Engine,
    keyboard: Keyboard,
    effects: Effects,
    timers: Timers,
    reader: Reader,
    worker: Worker,
    overlay: Overlay,
}

impl App {
    fn serve(&mut self, inbound: &Receiver<Inbound>) -> Result<(), RunError> {
        loop {
            let message = match self.timers.next() {
                Some(due) => inbound.recv_timeout(due.saturating_duration_since(Instant::now())),
                None => inbound.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match message {
                Ok(message) => match self.handle(message) {
                    Flow::Continue => {}
                    Flow::Stop => return Ok(()),
                    Flow::Fail(error) => return Err(error),
                },
                Err(RecvTimeoutError::Timeout) => self.fire_timers(),
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
    }

    fn fire_timers(&mut self) {
        for due in self.timers.take_due(Instant::now()) {
            match due {
                Due::Debounce(generation) => self.debounce_elapsed(generation),
                Due::KeyDeadline => self.fail_open(),
                Due::KeyPoll => self.poll_key(),
            }
        }
    }

    fn shutdown(mut self) {
        self.overlay.hide();
        self.keyboard.stop();
        drop(self.reader);
        drop(self.worker);
    }
}

fn forward_signals(inbox: &Inbox) -> io::Result<()> {
    let mut signals = Signals::new([SIGINT, SIGTERM])?;
    let inbox = inbox.clone();
    std::thread::spawn(move || {
        if signals.forever().next().is_some() {
            inbox.post(Inbound::Quit);
        }
    });
    Ok(())
}

pub fn run(
    config: Config,
    worker: Worker,
    inbox: Inbox,
    inbound: Receiver<Inbound>,
    options: Options,
) -> Result<(), RunError> {
    let display = Display::connect()?;
    let desktop = Desktop::new(display.clone())?;
    let grabs = Grabs::new(display.clone())?;
    let held = Held::default();
    let overlay = Overlay::create(display.clone(), desktop.clone(), config.ghost_opacity)?;
    let clipboard = Clipboard::spawn({
        let failures = inbox.clone();
        move |error| failures.post(Inbound::Failed(error))
    })
    .map_err(|error| log(&format!("clipboard unavailable: {error}")))
    .ok();
    let (keys, closed) = (inbox.clone(), inbox.clone());
    let _listener = listen(
        display,
        grabs.clone(),
        desktop.clone(),
        held.clone(),
        move |stroke| keys.post(Inbound::Key(stroke)),
        move |error| closed.post(Inbound::Failed(error)),
    );
    forward_signals(&inbox)?;
    let reports = inbox.clone();
    let reader = Reader::spawn(
        config.clone(),
        options.consent,
        desktop,
        held.clone(),
        Box::new(move |report| reports.post(Inbound::Report(report))),
    )?;
    let mut keyboard = Keyboard::new(&config.keymap(), grabs, held);
    keyboard.keys.set_trigger(config.enabled);
    let mut app = App {
        engine: Engine::new(config),
        keyboard,
        effects: Effects::new(options.insert, options.observer, clipboard),
        timers: Timers::default(),
        reader,
        worker,
        overlay,
    };
    let outcome = app.serve(&inbound);
    app.shutdown();
    outcome
}
