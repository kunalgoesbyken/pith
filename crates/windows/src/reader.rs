use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use pith_core::{Config, FieldSnapshot, Suffix, place};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationElement,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, GetForegroundWindow};
use windows::core::{Interface, Result};

use crate::caret::{
    caret_range, caret_rect, composing, font, has_selection, runtime_id, text_around,
};
use crate::util::{is_elevated, process_name, rect_of, window_pid, window_title, work_areas};

#[derive(Clone, Copy, Debug)]
pub enum Source {
    Focused,
    Window(&'static AtomicIsize),
}

const MAX_READ_ERRORS: u32 = 3;

static READABLE_OWN_FIELD: AtomicIsize = AtomicIsize::new(0);

pub fn allow_own_field(field: Option<HWND>) {
    let handle = field.map_or(0, |field| field.0 as isize);
    READABLE_OWN_FIELD.store(handle, Ordering::SeqCst);
}

fn is_readable_own_field(hwnd: HWND) -> bool {
    !hwnd.0.is_null() && hwnd.0 as isize == READABLE_OWN_FIELD.load(Ordering::SeqCst)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Poke {
    Changed,
    Key(u64),
}

impl Poke {
    fn key(self) -> Option<u64> {
        match self {
            Poke::Changed => None,
            Poke::Key(id) => Some(id),
        }
    }
}

pub type SnapshotCallback = Box<dyn Fn(Option<FieldSnapshot>, Option<u64>) + Send>;

pub struct Reader {
    pokes: Sender<Poke>,
    config: Arc<Mutex<Config>>,
}

impl Reader {
    pub fn spawn(source: Source, config: Config, on_snapshot: SnapshotCallback) -> Self {
        let (pokes, rx) = channel();
        let config = Arc::new(Mutex::new(config));
        let shared = config.clone();
        std::thread::Builder::new()
            .name("pith-uia".into())
            .spawn(move || serve(source, &shared, rx, on_snapshot))
            .expect("spawn uia thread");
        Self { pokes, config }
    }

    pub fn set_config(&self, config: &Config) {
        if let Ok(mut shared) = self.config.lock() {
            *shared = config.clone();
        }
        self.poke();
    }

    pub fn poke(&self) {
        let _ = self.pokes.send(Poke::Changed);
    }

    pub fn request(&self, id: u64) {
        let _ = self.pokes.send(Poke::Key(id));
    }

    pub fn sender(&self) -> Sender<Poke> {
        self.pokes.clone()
    }
}

fn automation() -> Result<IUIAutomation> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
        if let Ok(tuned) = automation.cast::<IUIAutomation2>() {
            let _ = tuned.SetConnectionTimeout(250);
            let _ = tuned.SetTransactionTimeout(250);
        }
        Ok(automation)
    }
}

fn serve(
    source: Source,
    config: &Mutex<Config>,
    rx: Receiver<Poke>,
    on_snapshot: SnapshotCallback,
) {
    let automation = match automation() {
        Ok(automation) => automation,
        Err(error) => return pith_core::log(&format!("uia unavailable: {error}")),
    };
    let own_pid = std::process::id();
    let mut last: Option<FieldSnapshot> = None;
    let mut first = true;
    let mut errors = 0;
    while let Ok(request) = rx.recv() {
        let served = std::iter::once(request)
            .chain(rx.try_iter())
            .filter_map(Poke::key)
            .max();
        let current = config
            .lock()
            .map(|shared| shared.clone())
            .unwrap_or_default();
        let outcome = if current.enabled {
            read(&automation, source, &current, own_pid)
        } else {
            Ok(None)
        };
        let snapshot = match outcome {
            Ok(snapshot) => {
                errors = 0;
                snapshot
            }
            Err(_) if errors + 1 < MAX_READ_ERRORS && served.is_none() => {
                errors += 1;
                continue;
            }
            Err(_) => None,
        };
        if first || served.is_some() || snapshot != last {
            first = false;
            last = snapshot.clone();
            on_snapshot(snapshot, served);
        }
    }
}

fn normalize(text: String) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text
    }
}

fn read(
    automation: &IUIAutomation,
    source: Source,
    config: &Config,
    own_pid: u32,
) -> Result<Option<FieldSnapshot>> {
    unsafe {
        let (element, root, foreground): (IUIAutomationElement, HWND, Option<HWND>) = match source {
            Source::Focused => {
                let foreground = GetForegroundWindow();
                (
                    automation.GetFocusedElement()?,
                    foreground,
                    Some(foreground),
                )
            }
            Source::Window(handle) => {
                let hwnd = HWND(handle.load(Ordering::SeqCst) as _);
                (
                    automation.ElementFromHandle(hwnd)?,
                    GetAncestor(hwnd, GA_ROOT),
                    None,
                )
            }
        };
        let pid = element.CurrentProcessId()? as u32;
        if matches!(source, Source::Focused)
            && pid == own_pid
            && !element
                .CurrentNativeWindowHandle()
                .is_ok_and(is_readable_own_field)
        {
            return Ok(None);
        }
        let owner = window_pid(root).unwrap_or(pid);
        if is_elevated(pid) || is_elevated(owner) {
            return Ok(None);
        }
        let app = process_name(owner);
        let blocked = app.as_deref().is_none_or(|name| config.is_blocked(name));
        let is_password = element
            .CurrentIsPassword()
            .map(|b| b.as_bool())
            .unwrap_or(true);
        let Ok(caret) = caret_range(&element) else {
            return Ok(None);
        };
        let (before, after) = if is_password || blocked {
            (String::new(), String::new())
        } else {
            let lookback = config.lookback_chars();
            text_around(&caret, i32::try_from(lookback).unwrap_or(i32::MAX), 200)?
        };
        let style = font(&caret, GetDpiForWindow(root).max(96));
        let element_rect = element.CurrentBoundingRectangle().ok().map(rect_of);
        let after = normalize(after);
        let placement = place(
            caret_rect(&caret, foreground),
            element_rect,
            &work_areas(),
            Suffix::of(&after),
        );
        Ok(Some(FieldSnapshot {
            field_id: runtime_id(&element),
            app: app.unwrap_or_default(),
            window_title: window_title(root),
            before: normalize(before),
            after,
            has_selection: has_selection(&element),
            is_password,
            composing: composing(&element),
            placement,
            style,
        }))
    }
}
