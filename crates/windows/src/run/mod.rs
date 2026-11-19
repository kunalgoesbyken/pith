mod commands;
mod effects;
mod inbox;
mod keyboard;
mod preferences;
mod presenter;

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

use pith_core::{Action, Config, Engine, ModelPreset};
use pith_infer::Worker;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::appearance::Appearance;
use crate::command::TrayCommand;
use crate::control::{Signal, control_window, next_signal};
use crate::erase::erase;
use crate::events::Hooks;
use crate::overlay::Overlay;
use crate::power;
use crate::reader::{Reader, Source};
use crate::spell::Speller;

use effects::Effects;
pub use inbox::{Inbound, Inbox};
use keyboard::Keyboard;
use preferences::Preferences;
use presenter::Presenter;

const TIMER_POLL: usize = 1;
const TIMER_DEBOUNCE: usize = 2;
const TIMER_REARM: usize = 3;
const TIMER_REPLACE: usize = 4;
const MIN_POLL_MS: u32 = 16;

pub enum InsertMode {
    Paste,
    Custom(Box<dyn Fn(String)>),
}

pub enum Start {
    Tray,
    Home,
    Onboarding(PathBuf),
}

pub struct Options {
    pub source: Source,
    pub insert: InsertMode,
    pub tray: bool,
    pub poll_ms: u32,
    pub config_path: Option<PathBuf>,
    pub observer: Option<Sender<Action>>,
    pub global_keys: bool,
    pub start: Start,
    pub data_dir: Option<PathBuf>,
    pub notice: Option<String>,
    pub recommended: ModelPreset,
}

struct App {
    control: HWND,
    engine: Engine,
    prefs: Preferences,
    keyboard: Keyboard,
    presenter: Presenter,
    effects: Effects,
    reader: Reader,
    worker: Worker,
    speller: Option<Speller>,
    overlay: Overlay,
    data_dir: Option<PathBuf>,
    erasing: Option<PathBuf>,
    inbox: Inbox,
    inbound: Receiver<Inbound>,
}

impl App {
    fn new(
        control: HWND,
        config: Config,
        reader: Reader,
        worker: Worker,
        (inbox, inbound): (Inbox, Receiver<Inbound>),
        options: Options,
    ) -> windows::core::Result<Self> {
        let appearance = Appearance::detect();
        let speller = Speller::spawn({
            let spelled = inbox.clone();
            move |generation, spelling| spelled.post(Inbound::Spelled(generation, spelling))
        });
        let presenter = Presenter::new(&config, &options, appearance);
        let prefs = Preferences::new(
            config.clone(),
            options.config_path,
            options.recommended,
            speller.is_some(),
        );
        let mut app = Self {
            control,
            overlay: Overlay::create(config.ghost_opacity, appearance)?,
            keyboard: Keyboard::new(&config.keymap(), control, options.global_keys),
            presenter,
            effects: Effects::new(options.insert, options.observer),
            engine: Engine::new(prefs.effective()),
            prefs,
            reader,
            worker,
            speller,
            data_dir: options.data_dir,
            erasing: None,
            inbox,
            inbound,
        };
        app.keyboard.set_trigger(config.enabled);
        if app.prefs.paused() {
            app.sync();
        }
        app.reader.poke();
        Ok(app)
    }

    fn set_timer(&self, timer: usize, milliseconds: u32) {
        unsafe {
            SetTimer(Some(self.control), timer, milliseconds, None);
        }
    }

    fn kill_timer(&self, timer: usize) {
        unsafe {
            let _ = KillTimer(Some(self.control), timer);
        }
    }

    fn signal(&mut self, signal: Signal) -> bool {
        match signal {
            Signal::Wake => return self.drain(),
            Signal::Timer(TIMER_POLL) => return self.poll(),
            Signal::Timer(TIMER_DEBOUNCE) => self.debounce_elapsed(),
            Signal::Timer(TIMER_REARM) => self.rearm(),
            Signal::Timer(TIMER_REPLACE) => self.replace_timed_out(),
            Signal::Timer(_) => {}
            Signal::Power => self.power_changed(),
            Signal::Appearance => self.appearance_changed(),
            Signal::Hotkey(id) => self.hotkey_pressed(id),
            Signal::Reveal => {
                self.command(TrayCommand::Open);
            }
        }
        false
    }

    fn appearance_changed(&mut self) {
        let appearance = Appearance::detect();
        self.overlay.set_appearance(appearance);
        self.presenter.set_appearance(appearance, &self.prefs);
    }

    fn poll(&mut self) -> bool {
        self.collect_stats();
        self.expire_pause();
        self.presenter.poll(&self.prefs);
        if self.engine.config().enabled {
            self.reader.poke();
        }
        self.drain()
    }

    fn serve(&mut self) -> bool {
        if self.ui_events() {
            return true;
        }
        while let Some(signal) = next_signal() {
            if self.signal(signal) {
                return true;
            }
        }
        self.presenter.finish_onboarding(&self.prefs);
        self.tray_commands()
    }

    fn pump(&mut self) {
        let mut message = MSG::default();
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
            if !self.presenter.handles_dialog(&message) {
                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            if self.serve() {
                break;
            }
        }
    }

    fn shutdown(mut self) {
        self.collect_stats();
        if self.erasing.is_none() {
            self.presenter.flush_stats();
        }
        self.overlay.hide();
        self.keyboard.stop();
        match self.erasing {
            Some(dir) => erase(&dir, self.worker),
            None => drop(self.worker),
        }
    }
}

pub fn run(
    config: Config,
    worker: Worker,
    inbox: Inbox,
    inbound: Receiver<Inbound>,
    options: Options,
) -> windows::core::Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let control = control_window()?;
    inbox.attach(control);
    let snapshots = inbox.clone();
    let reader = Reader::spawn(
        options.source,
        config.clone(),
        Box::new(move |snapshot, served| snapshots.post(Inbound::Snapshot(snapshot, served))),
    );
    let _hooks = Hooks::install(reader.sender());
    let _power_watch = power::watch(control);
    unsafe {
        SetTimer(
            Some(control),
            TIMER_POLL,
            options.poll_ms.max(MIN_POLL_MS),
            None,
        );
    }
    let open_home = matches!(options.start, Start::Home);
    let mut app = App::new(control, config, reader, worker, (inbox, inbound), options)?;
    if open_home {
        app.command(TrayCommand::Open);
    }
    app.pump();
    app.shutdown();
    Ok(())
}
