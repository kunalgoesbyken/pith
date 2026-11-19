mod commands;
mod effects;
mod inbox;
mod keyboard;
mod shell;
mod style;
mod timers;

use std::fmt;
use std::io;
use std::path::PathBuf;
#[cfg(feature = "e2e")]
use std::sync::mpsc::Sender;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Instant;

#[cfg(feature = "e2e")]
use pith_core::Action;
use pith_core::{Config, Engine, ModelPreset, Preferences, StyleStore, log};
use pith_infer::Worker;
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use crate::accessibility::Consent;
use crate::autostart::Autostart;
use crate::current::Current;
use crate::error::ReaderError;
use crate::held::Held;
use crate::overlay::Overlay;
use crate::reader::Reader;
use crate::session::Quit;
use crate::tray::Tray;
use crate::x11::{Clipboard, Desktop, Display, DisplayError, Grabs, listen};

use effects::Effects;
use inbox::Flow;
pub use inbox::{Inbound, Inbox};
use keyboard::Keyboard;
use shell::Shell;
use timers::{Due, Timers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertPolicy {
    Auto,
    #[cfg(feature = "e2e")]
    Accessibility,
    #[cfg(feature = "e2e")]
    Clipboard,
}

#[cfg(feature = "e2e")]
impl InsertPolicy {
    fn via_accessibility(self) -> bool {
        self != Self::Clipboard
    }

    fn falls_back(self) -> bool {
        self == Self::Auto
    }
}

#[cfg(not(feature = "e2e"))]
impl InsertPolicy {
    fn via_accessibility(self) -> bool {
        true
    }

    fn falls_back(self) -> bool {
        true
    }
}

pub enum Start {
    Tray,
    Home,
    Welcome(PathBuf),
}

pub struct Options {
    pub consent: Option<Consent>,
    pub data_dir: Option<PathBuf>,
    pub config_path: PathBuf,
    pub recommended: ModelPreset,
    pub start: Start,
    pub notice: Option<String>,
    pub autostart: Option<Autostart>,
    #[cfg(feature = "e2e")]
    pub insert: InsertPolicy,
    #[cfg(feature = "e2e")]
    pub observer: Option<Sender<Action>>,
}

impl Options {
    pub fn new(consent: Option<Consent>, config_path: PathBuf, recommended: ModelPreset) -> Self {
        Self {
            consent,
            data_dir: None,
            config_path,
            recommended,
            start: Start::Tray,
            notice: None,
            autostart: None,
            #[cfg(feature = "e2e")]
            insert: InsertPolicy::Auto,
            #[cfg(feature = "e2e")]
            observer: None,
        }
    }
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
    prefs: Preferences,
    shell: Shell,
    keyboard: Keyboard,
    effects: Effects,
    timers: Timers,
    reader: Reader,
    worker: Worker,
    overlay: Overlay,
    current: Current,
    style: Option<StyleStore>,
}

impl App {
    fn serve(&mut self, inbound: &Receiver<Inbound>) -> Result<(), RunError> {
        loop {
            self.fire_timers();
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
                Due::PauseCheck => self.expire_pause(),
            }
        }
    }

    fn shutdown(mut self) {
        self.fail_open();
        self.overlay.hide();
        self.keyboard.stop();
        self.effects.finish();
        drop(self.reader);
        drop(self.worker);
    }
}

fn forward_signals(inbox: &Inbox, quit: &Quit) -> io::Result<()> {
    let mut signals = Signals::new([SIGHUP, SIGINT, SIGTERM])?;
    let (inbox, quit) = (inbox.clone(), quit.clone());
    std::thread::spawn(move || {
        for _ in signals.forever() {
            quit.request();
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
    let current = Current::default();
    let clipboard = Clipboard::spawn(
        current.clone(),
        {
            let unpasted = inbox.clone();
            move || unpasted.post(Inbound::Unpasted)
        },
        {
            let failures = inbox.clone();
            move |error| failures.post(Inbound::Failed(error))
        },
    )
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
    let quit = Quit::default();
    forward_signals(&inbox, &quit)?;
    let reports = inbox.clone();
    let reader = match Reader::spawn(
        config.clone(),
        options.consent,
        desktop,
        held.clone(),
        &quit,
        Box::new(move |report| reports.post(Inbound::Report(report))),
    ) {
        Err(ReaderError::Stopped) if quit.is_requested() => return Ok(()),
        outcome => outcome?,
    };
    let keyboard = Keyboard::new(&config.keymap(), grabs, held);
    let tray = Tray::spawn(inbox.clone(), options.autostart)
        .map_err(|error| log(&format!("no tray icon: {error}")))
        .ok();
    let prefs = Preferences::new(
        config,
        Some(options.config_path),
        options.recommended,
        false,
    );
    #[cfg(feature = "e2e")]
    let effects = Effects::new(options.insert, options.observer, clipboard);
    #[cfg(not(feature = "e2e"))]
    let effects = Effects::new(InsertPolicy::Auto, clipboard);
    let style = style::open(options.data_dir.as_deref(), &config);
    let mut app = App {
        engine: Engine::new(prefs.effective()),
        prefs,
        shell: Shell::new(tray, options.notice),
        keyboard,
        effects,
        timers: Timers::default(),
        reader,
        worker,
        overlay,
        current,
        style,
    };
    app.share_style();
    app.begin(options.start);
    let outcome = app.serve(&inbound);
    app.shutdown();
    outcome
}
