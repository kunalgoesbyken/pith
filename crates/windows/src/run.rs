use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant, SystemTime};

use pith_core::{
    Action, Chord, Completion, Config, Engine, Event, FieldSnapshot, Key, Model, Pause, PauseSpan,
    Setting, SkipReason, Status, check, clock_after, log,
};
use pith_infer::Worker;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use crate::clipboard::{Trigger, paste};
use crate::control::{Signal, Waker, control_window, next_signal};
use crate::erase::erase;
use crate::events::Hooks;
use crate::foreground::Foreground;
use crate::keys::{KeyAction, Keys, replay};
use crate::onboarding::{Event as OnboardingEvent, Onboarding, Outcome};
use crate::overlay::Overlay;
use crate::reader::{Reader, Source};
use crate::registry;
use crate::settings::{SettingsWindow, UiEvent, take_events};
use crate::tray::{Tray, TrayCommand, TrayView};

const TIMER_POLL: usize = 1;
const TIMER_DEBOUNCE: usize = 2;
const TIMER_REARM: usize = 3;
const REARM_MS: u32 = 60;

pub enum InsertMode {
    Paste,
    Custom(Box<dyn Fn(String)>),
}

pub struct Options {
    pub source: Source,
    pub insert: InsertMode,
    pub tray: bool,
    pub poll_ms: u32,
    pub config_path: Option<PathBuf>,
    pub observer: Option<Sender<Action>>,
    pub global_keys: bool,
    pub onboarding: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub notice: Option<String>,
}

pub enum Inbound {
    Snapshot(Option<FieldSnapshot>, u64),
    Done(u64, Result<Completion, String>),
    Ready(Result<String, String>),
    Status(String),
    Progress(u64, u64),
    Key(KeyAction),
    SetEnabled(bool),
    Resume(String),
    Quit,
}

#[derive(Clone, Copy)]
struct Press {
    action: KeyAction,
    chord: Option<Chord>,
    at: Instant,
}

impl Press {
    fn moves_caret(&self) -> bool {
        self.chord.is_some_and(|chord| chord.key == Key::Right)
    }
}

struct ActivePause {
    shown: Pause,
    ends: Option<SystemTime>,
}

struct App {
    control: HWND,
    engine: Engine,
    saved: Config,
    pause: Option<ActivePause>,
    model: Model,
    skip: Option<SkipReason>,
    foreground: Foreground,
    settings: Option<SettingsWindow>,
    delete_data: bool,
    worker: Worker,
    overlay: Overlay,
    keys: Keys<HWND>,
    reader: Reader,
    tray: Option<Tray>,
    onboarding: Option<Onboarding>,
    options: Options,
    inbound: Receiver<Inbound>,
    pending_debounce: u64,
    started: Option<Instant>,
    keys_awaiting_read: Vec<Press>,
    key_read: u64,
}

impl App {
    fn apply(&mut self, actions: Vec<Action>) {
        for action in actions {
            if let Some(observer) = &self.options.observer {
                let _ = observer.send(action.clone());
            }
            match action {
                Action::ScheduleDebounce {
                    generation,
                    delay_ms,
                } => {
                    self.pending_debounce = generation;
                    unsafe {
                        SetTimer(Some(self.control), TIMER_DEBOUNCE, delay_ms as u32, None);
                    }
                }
                Action::StartCompletion {
                    generation,
                    request,
                } => {
                    self.started = Some(Instant::now());
                    self.worker.submit(generation, request);
                }
                Action::CancelCompletion => self.worker.cancel(),
                Action::Show {
                    text,
                    caret,
                    font_family,
                    font_px,
                    color,
                } => self
                    .overlay
                    .show(&text, caret, font_family.as_deref(), font_px, color),
                Action::Hide => self.overlay.hide(),
                Action::Insert(text) => match &self.options.insert {
                    InsertMode::Paste => paste(text, Trigger::Chord),
                    InsertMode::Custom(insert) => insert(text),
                },
                Action::AutoPaused(apps) => {
                    log(&format!("auto-paused in {apps:?}"));
                    if let Some(tray) = &self.tray {
                        tray.set_auto_paused(&apps);
                    }
                }
                Action::ArmKeys if self.options.global_keys => self.keys.arm(),
                Action::ArmKeys => {}
                Action::DisarmKeys => self.keys.disarm(),
            }
        }
    }

    fn drain(&mut self) -> bool {
        let mut quit = false;
        while let Ok(message) = self.inbound.try_recv() {
            match message {
                Inbound::Snapshot(snapshot, served) => {
                    self.skip = snapshot
                        .as_ref()
                        .and_then(|field| check(field, self.engine.config()).err());
                    self.refresh_tray();
                    let actions = self.engine.handle(Event::Field(snapshot));
                    self.apply(actions);
                    if served >= self.key_read {
                        self.release_keys();
                    }
                }
                Inbound::Done(generation, result) => {
                    if let (Some(started), Ok(completion)) = (self.started.take(), &result) {
                        log(&format!(
                            "completion {} ms, {} tokens, {} prompt tokens, logprob {:.2}, {:?}",
                            started.elapsed().as_millis(),
                            completion.tokens,
                            completion.prompt_tokens_evaluated,
                            completion.mean_logprob,
                            completion.stop
                        ));
                    }
                    if let Err(error) = &result {
                        log(&format!("completion error: {error}"));
                    }
                    let actions = self.engine.handle(Event::Completed { generation, result });
                    self.apply(actions);
                }
                Inbound::Ready(status) => {
                    match &status {
                        Ok(name) => log(&format!("ready ({name})")),
                        Err(error) => log(&format!("error: {error}")),
                    }
                    self.model = match &status {
                        Ok(_) => Model::Ready,
                        Err(error) => Model::Failed(error.clone()),
                    };
                    self.refresh_tray();
                    self.onboard(match status {
                        Ok(_) => OnboardingEvent::Ready,
                        Err(error) => OnboardingEvent::Failed(error),
                    });
                }
                Inbound::Status(text) => {
                    log(&text);
                    self.model = Model::Loading(text);
                    self.refresh_tray();
                    self.onboard(OnboardingEvent::Loading);
                }
                Inbound::Progress(done, total) => {
                    self.onboard(OnboardingEvent::Progress(done, total))
                }
                Inbound::Key(action) => self.hotkey(Press {
                    action,
                    chord: None,
                    at: Instant::now(),
                }),
                Inbound::SetEnabled(enabled) => self.set_enabled(enabled),
                Inbound::Resume(app) => self.resume_app(app),
                Inbound::Quit => quit = true,
            }
        }
        quit
    }

    fn onboard(&mut self, event: OnboardingEvent) {
        if let Some(onboarding) = &mut self.onboarding {
            onboarding.event(event);
        }
    }

    fn onboarding_outcome(&mut self) {
        let Some(onboarding) = &mut self.onboarding else {
            return;
        };
        if onboarding.poll() == Outcome::Pending {
            return;
        }
        self.onboarding = None;
        if let Some(tray) = &self.tray {
            tray.balloon("Pith", "Pith is running in the tray");
        }
    }

    fn hotkey(&mut self, press: Press) {
        let needs_field = press.action == KeyAction::Trigger;
        if !needs_field && self.engine.suggestion().is_none() && self.keys_awaiting_read.is_empty()
        {
            self.pass_through(press);
            return;
        }
        self.keys_awaiting_read.push(press);
        self.key_read += 1;
        self.reader.request(self.key_read);
    }

    fn release_keys(&mut self) {
        for press in std::mem::take(&mut self.keys_awaiting_read) {
            let handled = match press.action {
                KeyAction::Trigger => true,
                KeyAction::Dismiss => self.engine.suggestion().is_some(),
                KeyAction::AcceptAll | KeyAction::AcceptWord => {
                    self.engine.can_accept(press.at, press.moves_caret())
                }
            };
            if handled {
                self.key(press.action);
            } else {
                self.pass_through(press);
            }
        }
    }

    fn pass_through(&mut self, press: Press) {
        let Some(chord) = press.chord.filter(|_| self.options.global_keys) else {
            return;
        };
        self.keys.disarm();
        replay(&chord);
        if self.engine.suggestion().is_some() {
            unsafe {
                SetTimer(Some(self.control), TIMER_REARM, REARM_MS, None);
            }
        }
    }

    fn signal(&mut self, signal: Signal) -> bool {
        match signal {
            Signal::Wake => return self.drain(),
            Signal::Timer(TIMER_POLL) => {
                self.expire_pause();
                if self.tray.is_some() {
                    self.foreground.poll();
                    self.refresh_tray();
                }
                if self.engine.config().enabled {
                    self.reader.poke();
                }
                return self.drain();
            }
            Signal::Timer(TIMER_DEBOUNCE) => {
                unsafe {
                    let _ = KillTimer(Some(self.control), TIMER_DEBOUNCE);
                }
                let actions = self
                    .engine
                    .handle(Event::DebounceElapsed(self.pending_debounce));
                self.apply(actions);
            }
            Signal::Timer(TIMER_REARM) => {
                unsafe {
                    let _ = KillTimer(Some(self.control), TIMER_REARM);
                }
                if self.options.global_keys && self.engine.suggestion().is_some() {
                    self.keys.arm();
                }
            }
            Signal::Timer(_) => {}
            Signal::Hotkey(id) => {
                if let Some(binding) = self.keys.binding(id) {
                    self.hotkey(Press {
                        action: binding.action,
                        chord: Some(binding.chord),
                        at: Instant::now(),
                    });
                }
            }
        }
        false
    }

    fn key(&mut self, action: KeyAction) {
        let actions = self.engine.handle(action.event());
        self.apply(actions);
    }

    fn effective(&self) -> Config {
        Config {
            enabled: self.saved.enabled && self.pause.is_none(),
            ..self.saved.clone()
        }
    }

    fn sync(&mut self) {
        let config = self.effective();
        self.keys.rebind(&config.keymap());
        self.reader.set_config(&config);
        let enabled = config.enabled;
        let actions = self.engine.handle(Event::SetConfig(config));
        self.apply(actions);
        self.sync_trigger(enabled);
        self.refresh_tray();
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.saved.enabled = enabled;
        self.sync();
    }

    fn resume_app(&mut self, app: String) {
        let actions = self.engine.handle(Event::Resume(app));
        self.apply(actions);
    }

    fn sync_trigger(&mut self, enabled: bool) {
        if self.options.global_keys {
            self.keys.set_trigger(enabled);
        }
    }

    fn status_line(&self) -> String {
        Status {
            model: self.model.clone(),
            pause: self.pause.as_ref().map(|pause| pause.shown.clone()),
            enabled: self.saved.enabled,
            skip: self.skip,
            notice: self.options.notice.clone(),
        }
        .line()
    }

    fn refresh_tray(&self) {
        self.refresh_settings();
        let Some(tray) = &self.tray else { return };
        let app = self.foreground.app();
        tray.show(TrayView {
            status: self.status_line(),
            active: self.engine.config().enabled,
            paused: self.pause.is_some(),
            foreground: app.map(str::to_string),
            foreground_blocked: app.is_some_and(|name| self.saved.is_blocked(name)),
        });
    }

    fn pause(&mut self, span: PauseSpan) {
        self.pause = Some(match span {
            PauseSpan::UntilResumed => ActivePause {
                shown: Pause::UntilResumed,
                ends: None,
            },
            PauseSpan::Minutes(minutes) => {
                let now = unsafe { GetLocalTime() };
                ActivePause {
                    shown: Pause::Until(clock_after(
                        u32::from(now.wHour),
                        u32::from(now.wMinute),
                        minutes,
                    )),
                    ends: SystemTime::now()
                        .checked_add(Duration::from_secs(u64::from(minutes) * 60)),
                }
            }
        });
        self.sync();
    }

    fn resume(&mut self) {
        if self.pause.take().is_some() {
            self.sync();
        }
    }

    fn expire_pause(&mut self) {
        let due = self
            .pause
            .as_ref()
            .and_then(|pause| pause.ends)
            .is_some_and(|ends| SystemTime::now() >= ends);
        if due {
            self.resume();
        }
    }

    fn change(&mut self, setting: Setting) {
        setting.apply(&mut self.saved);
        self.persist(&setting);
        self.sync();
    }

    fn persist(&self, setting: &Setting) {
        let Some(path) = &self.options.config_path else {
            return;
        };
        if let Err(error) = setting.persist(path) {
            log(&format!("settings not saved: {error}"));
        }
    }

    fn refresh_settings(&self) {
        if let Some(window) = self.settings.as_ref().filter(|window| window.is_open()) {
            window.refresh(
                &self.saved,
                registry::start_with_windows(),
                &self.status_line(),
            );
        }
    }

    fn open_settings(&mut self) {
        if let Some(window) = self.settings.as_ref().filter(|window| window.is_open()) {
            window.show();
            return;
        }
        match SettingsWindow::open(
            &self.saved,
            registry::start_with_windows(),
            &self.status_line(),
        ) {
            Ok(window) => {
                window.show();
                self.settings = Some(window);
            }
            Err(error) => log(&format!("settings window failed: {error}")),
        }
    }

    fn confirm_delete(&self) -> bool {
        let owner = self.settings.as_ref().map(SettingsWindow::hwnd);
        let answer = unsafe {
            MessageBoxW(
                owner,
                w!(
                    "Delete the downloaded model and the local data of Pith from this PC, then close Pith? Your settings are kept. The model downloads again the next time Pith starts."
                ),
                w!("Delete model and data"),
                MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
            )
        };
        answer == IDYES
    }

    fn ui_events(&mut self) -> bool {
        for event in take_events() {
            match event {
                UiEvent::Apply(setting) => self.change(setting),
                UiEvent::OpenDataFolder => {
                    if let Some(dir) = &self.options.data_dir {
                        let _ = std::fs::create_dir_all(dir);
                        crate::util::open_path(dir);
                    }
                }
                UiEvent::DeleteData => {
                    if self.options.data_dir.is_some() && self.confirm_delete() {
                        self.delete_data = true;
                        return true;
                    }
                }
                UiEvent::StartWithWindows(enabled) => {
                    if let Err(error) = registry::set_start_with_windows(enabled) {
                        log(&format!("start with Windows not changed: {error}"));
                    }
                    self.refresh_settings();
                }
                UiEvent::Closed => self.settings = None,
            }
        }
        false
    }

    fn tray_command(&mut self) -> bool {
        while let Some(command) = self.tray.as_ref().and_then(Tray::poll) {
            match command {
                TrayCommand::Pause(span) => self.pause(span),
                TrayCommand::Resume => self.resume(),
                TrayCommand::ResumeApp(app) => self.resume_app(app),
                TrayCommand::BlockForeground => {
                    if let Some(app) = self.foreground.app().map(str::to_string) {
                        self.change(Setting::Block(app));
                    }
                }
                TrayCommand::Settings => self.open_settings(),
                TrayCommand::Quit => return true,
            }
        }
        false
    }
}

pub fn run(
    config: Config,
    worker: Worker,
    inbound: (Sender<Inbound>, Receiver<Inbound>),
    waker: Waker,
    options: Options,
) -> windows::core::Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let control = control_window()?;
    waker.attach(control);
    let (sender, receiver) = inbound;
    let snapshot_sender = sender.clone();
    let snapshot_waker = waker.clone();
    let reader = Reader::spawn(
        options.source,
        config.clone(),
        Box::new(move |snapshot, served| {
            let _ = snapshot_sender.send(Inbound::Snapshot(snapshot, served));
            snapshot_waker.wake();
        }),
    );
    let _hooks = Hooks::install(reader.sender());
    unsafe {
        SetTimer(Some(control), TIMER_POLL, options.poll_ms.max(16), None);
    }
    let tray = if options.tray {
        Tray::create(config.enabled)
    } else {
        None
    };
    let onboarding = options.onboarding.clone().and_then(|marker| {
        Onboarding::create(marker, true)
            .map_err(|error| log(&format!("welcome window failed: {error}")))
            .ok()
    });
    let keymap = config.keymap();
    let enabled = config.enabled;
    let mut app = App {
        control,
        overlay: Overlay::create(config.ghost_opacity)?,
        saved: config.clone(),
        pause: None,
        model: Model::Loading(String::new()),
        skip: None,
        foreground: Foreground::default(),
        settings: None,
        delete_data: false,
        engine: Engine::new(config),
        worker,
        keys: Keys::new(&keymap, control),
        reader,
        tray,
        onboarding,
        options,
        inbound: receiver,
        pending_debounce: 0,
        started: None,
        keys_awaiting_read: Vec::new(),
        key_read: 0,
    };
    app.sync_trigger(enabled);
    app.reader.poke();
    let mut message = MSG::default();
    'outer: while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
        let handled = app
            .settings
            .as_ref()
            .is_some_and(|window| unsafe { IsDialogMessageW(window.hwnd(), &message) }.as_bool());
        if !handled {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        if app.ui_events() {
            break;
        }
        while let Some(signal) = next_signal() {
            if app.signal(signal) {
                break 'outer;
            }
        }
        app.onboarding_outcome();
        if app.tray_command() {
            break;
        }
    }
    app.overlay.hide();
    app.keys.disarm();
    app.keys.set_trigger(false);
    match app
        .delete_data
        .then(|| app.options.data_dir.clone())
        .flatten()
    {
        Some(dir) => erase(&dir, app.worker),
        None => drop(app.worker),
    }
    Ok(())
}
