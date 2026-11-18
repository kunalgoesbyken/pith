use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use nib_core::{Action, Completion, Config, Engine, Event, FieldSnapshot, log};
use nib_infer::Worker;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use crate::clipboard::{paste, replace_selection};
use crate::events::Hooks;
use crate::keys::{ACCEPT_ALL, ACCEPT_WORD, DISMISS, Keys, replay};
use crate::overlay::Overlay;
use crate::reader::{Reader, Source};
use crate::tray::{Tray, TrayCommand};

const WM_WAKE: u32 = WM_APP + 1;
const TIMER_POLL: usize = 1;
const TIMER_DEBOUNCE: usize = 2;

pub enum InsertMode {
    Paste,
    Window(isize),
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
    Snapshot(Option<FieldSnapshot>),
    Done(u64, Result<Completion, String>),
    Ready(Result<String, String>),
    Status(String),
    Key(i32),
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
        let class = w!("NibControl");
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
            w!("nib-control"),
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
                    InsertMode::Window(hwnd) => replace_selection(hwnd, &text),
                },
                Action::ArmKeys if self.options.global_keys => self.keys.arm(self.control),
                Action::ArmKeys => {}
                Action::DisarmKeys => self.keys.disarm(self.control),
            }
        }
    }

    fn drain(&mut self) -> bool {
        let mut latest_snapshot = None;
        let mut quit = false;
        while let Ok(message) = self.inbound.try_recv() {
            match message {
                Inbound::Snapshot(snapshot) => latest_snapshot = Some(snapshot),
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
                Inbound::Key(id) => self.key(id),
                Inbound::Quit => quit = true,
            }
        }
        if let Some(snapshot) = latest_snapshot {
            let actions = self.engine.handle(Event::Field(snapshot));
            self.apply(actions);
        }
        quit
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
            Signal::Timer(_) => {}
            Signal::Hotkey(id) => {
                if self.engine.suggestion().is_none() {
                    self.keys.disarm(self.control);
                    replay(id);
                    return false;
                }
                self.key(id);
            }
        }
        false
    }

    fn key(&mut self, id: i32) {
        let event = match id {
            ACCEPT_ALL => Event::AcceptAll,
            ACCEPT_WORD => Event::AcceptWord,
            DISMISS => Event::Dismiss,
            _ => return,
        };
        let actions = self.engine.handle(event);
        self.apply(actions);
    }

    fn tray_command(&mut self) -> bool {
        let Some(command) = self.tray.as_ref().and_then(Tray::poll) else {
            return false;
        };
        match command {
            TrayCommand::Toggle => {
                let enabled = !self.engine.config().enabled;
                let actions = self.engine.handle(Event::SetEnabled(enabled));
                self.apply(actions);
                if let Some(tray) = &self.tray {
                    tray.set_enabled(enabled);
                }
            }
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
        config.context_chars,
        Box::new(move |snapshot| {
            let _ = snapshot_sender.send(Inbound::Snapshot(snapshot));
            snapshot_waker.wake();
        }),
    );
    let _hooks = Hooks::install(reader.sender());
    unsafe {
        SetTimer(Some(control), TIMER_POLL, options.poll_ms.max(16), None);
    }
    let tray = if options.tray { Tray::create() } else { None };
    let mut app = App {
        control,
        engine: Engine::new(config),
        worker,
        overlay: Overlay::create()?,
        keys: Keys::new(),
        reader,
        tray,
        options,
        inbound: receiver,
        pending_debounce: 0,
        started: None,
    };
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
