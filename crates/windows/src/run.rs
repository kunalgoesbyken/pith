use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use pith_core::{Action, Chord, Completion, Config, Engine, Event, FieldSnapshot, Key, log};
use pith_infer::Worker;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use crate::clipboard::{paste, replace_selection, replace_selection_after};
use crate::events::Hooks;
use crate::keys::{ACCEPT_ALL, ACCEPT_WORD, DISMISS, Keys, TRIGGER, replay};
use crate::overlay::Overlay;
use crate::reader::{Reader, Source};
use crate::tray::{Tray, TrayCommand};

const WM_WAKE: u32 = WM_APP + 1;
const TIMER_POLL: usize = 1;
const TIMER_DEBOUNCE: usize = 2;
const TIMER_REARM: usize = 3;
const REARM_MS: u32 = 60;

pub enum InsertMode {
    Paste,
    Window(&'static AtomicIsize),
    Lagging(&'static AtomicIsize, Duration),
}

pub struct Options {
    pub source: Source,
    pub insert: InsertMode,
    pub tray: bool,
    pub poll_ms: u32,
    pub config_path: Option<PathBuf>,
    pub observer: Option<Sender<Action>>,
    pub global_keys: bool,
}

pub enum Inbound {
    Snapshot(Option<FieldSnapshot>, u64),
    Done(u64, Result<Completion, String>),
    Ready(Result<String, String>),
    Status(String),
    Key(i32),
    SetEnabled(bool),
    Quit,
}

#[derive(Clone, Default)]
pub struct Waker(Arc<AtomicIsize>);

impl Waker {
    pub fn wake(&self) {
        let hwnd = self.0.load(Ordering::SeqCst);
        if hwnd != 0 {
            unsafe {
                let _ = PostMessageW(Some(HWND(hwnd as _)), WM_WAKE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Press {
    action: i32,
    chord: Option<Chord>,
    at: Instant,
}

impl Press {
    fn moves_caret(&self) -> bool {
        self.chord.is_some_and(|chord| chord.key == Key::Right)
    }
}

enum Signal {
    Wake,
    Timer(usize),
    Hotkey(i32),
}

thread_local! {
    static SIGNALS: RefCell<VecDeque<Signal>> = const { RefCell::new(VecDeque::new()) };
}

unsafe extern "system" fn control_procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let signal = match message {
        WM_WAKE => Some(Signal::Wake),
        WM_TIMER => Some(Signal::Timer(wparam.0)),
        WM_HOTKEY => Some(Signal::Hotkey(wparam.0 as i32)),
        _ => None,
    };
    if let Some(signal) = signal {
        SIGNALS.with(|queue| queue.borrow_mut().push_back(signal));
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn control_window() -> windows::core::Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = w!("PithControl");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(control_procedure),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("pith-control"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        )
    }
}

struct App {
    control: HWND,
    engine: Engine,
    worker: Worker,
    overlay: Overlay,
    keys: Keys,
    reader: Reader,
    tray: Option<Tray>,
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
                Action::Insert(text) => match self.options.insert {
                    InsertMode::Paste => paste(text),
                    InsertMode::Window(hwnd) => {
                        replace_selection(hwnd.load(Ordering::SeqCst), &text)
                    }
                    InsertMode::Lagging(hwnd, lag) => {
                        replace_selection_after(hwnd.load(Ordering::SeqCst), text, lag)
                    }
                },
                Action::ArmKeys if self.options.global_keys => self.keys.arm(self.control),
                Action::ArmKeys => {}
                Action::DisarmKeys => self.keys.disarm(self.control),
            }
        }
    }

    fn drain(&mut self) -> bool {
        let mut quit = false;
        while let Ok(message) = self.inbound.try_recv() {
            match message {
                Inbound::Snapshot(snapshot, served) => {
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
                    let text = match &status {
                        Ok(name) => format!("ready ({name})"),
                        Err(error) => format!("error: {error}"),
                    };
                    log(&text);
                    if let Some(tray) = &self.tray {
                        tray.set_status(&text);
                    }
                }
                Inbound::Status(text) => {
                    log(&text);
                    if let Some(tray) = &self.tray {
                        tray.set_status(&text);
                    }
                }
                Inbound::Key(action) => self.hotkey(Press {
                    action,
                    chord: None,
                    at: Instant::now(),
                }),
                Inbound::SetEnabled(enabled) => self.set_enabled(enabled),
                Inbound::Quit => quit = true,
            }
        }
        quit
    }

    fn hotkey(&mut self, press: Press) {
        let needs_field = press.action == TRIGGER;
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
                TRIGGER => true,
                DISMISS => self.engine.suggestion().is_some(),
                _ => self.engine.can_accept(press.at, press.moves_caret()),
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
        self.keys.disarm(self.control);
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
                    self.keys.arm(self.control);
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

    fn key(&mut self, id: i32) {
        let event = match id {
            ACCEPT_ALL => Event::AcceptAll,
            ACCEPT_WORD => Event::AcceptWord,
            DISMISS => Event::Dismiss,
            TRIGGER => Event::Trigger,
            _ => return,
        };
        let actions = self.engine.handle(event);
        self.apply(actions);
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.reader.set_enabled(enabled);
        self.sync_trigger(enabled);
        let actions = self.engine.handle(Event::SetEnabled(enabled));
        self.apply(actions);
        if let Some(tray) = &self.tray {
            tray.set_enabled(enabled);
        }
    }

    fn sync_trigger(&mut self, enabled: bool) {
        if self.options.global_keys {
            self.keys.set_trigger(self.control, enabled);
        }
    }

    fn tray_command(&mut self) -> bool {
        let Some(command) = self.tray.as_ref().and_then(Tray::poll) else {
            return false;
        };
        match command {
            TrayCommand::Toggle => self.set_enabled(!self.engine.config().enabled),
            TrayCommand::OpenConfig => {
                if let Some(path) = &self.options.config_path {
                    crate::util::open_path(path);
                }
            }
            TrayCommand::Quit => return true,
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
    waker.0.store(control.0 as isize, Ordering::SeqCst);
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
    let keymap = config
        .keymap()
        .or_else(|_| Config::default().keymap())
        .unwrap_or_default();
    let enabled = config.enabled;
    let mut app = App {
        control,
        engine: Engine::new(config),
        worker,
        overlay: Overlay::create()?,
        keys: Keys::new(&keymap),
        reader,
        tray,
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
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        while let Some(signal) = SIGNALS.with(|queue| queue.borrow_mut().pop_front()) {
            if app.signal(signal) {
                break 'outer;
            }
        }
        if app.tray_command() {
            break;
        }
    }
    app.overlay.hide();
    app.keys.disarm(app.control);
    Ok(())
}
