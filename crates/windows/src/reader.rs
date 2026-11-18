use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use pith_core::{Config, FieldSnapshot};
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
use crate::util::{is_elevated, process_name, window_pid, window_title};

#[derive(Clone, Copy, Debug)]
pub enum Source {
    Focused,
    Window(&'static AtomicIsize),
}

const MAX_READ_ERRORS: u32 = 3;

pub type SnapshotCallback = Box<dyn Fn(Option<FieldSnapshot>, u64) + Send>;

pub struct Reader {
    pokes: Sender<u64>,
    enabled: Arc<AtomicBool>,
    config: Arc<Mutex<Config>>,
}

impl Reader {
    pub fn spawn(source: Source, config: Config, on_snapshot: SnapshotCallback) -> Self {
        let (pokes, rx) = channel();
        let enabled = Arc::new(AtomicBool::new(config.enabled));
        let gate = enabled.clone();
        let config = Arc::new(Mutex::new(config));
        let shared = config.clone();
        std::thread::Builder::new()
            .name("pith-uia".into())
            .spawn(move || serve(source, &shared, rx, &gate, on_snapshot))
            .expect("spawn uia thread");
        Self {
            pokes,
            enabled,
            config,
        }
    }

    pub fn set_config(&self, config: &Config) {
        if let Ok(mut shared) = self.config.lock() {
            *shared = config.clone();
        }
        self.set_enabled(config.enabled);
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
        self.poke();
    }

    pub fn poke(&self) {
        let _ = self.pokes.send(0);
    }

    pub fn request(&self, id: u64) {
        let _ = self.pokes.send(id);
    }

    pub fn sender(&self) -> Sender<u64> {
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
    rx: Receiver<u64>,
    enabled: &AtomicBool,
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
        let served = rx.try_iter().fold(request, u64::max);
        let outcome = if enabled.load(Ordering::SeqCst) {
            let current = config.lock().map(|shared| shared.clone()).unwrap_or_default();
            read(&automation, source, &current, own_pid)
        } else {
            Ok(None)
        };
        let snapshot = match outcome {
            Ok(snapshot) => {
                errors = 0;
                snapshot
            }
            Err(_) if errors + 1 < MAX_READ_ERRORS && served == 0 => {
                errors += 1;
                continue;
            }
            Err(_) => None,
        };
        if first || served > 0 || snapshot != last {
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
        if matches!(source, Source::Focused) && pid == own_pid {
            return Ok(None);
        }
        let owner = window_pid(root).unwrap_or(pid);
        if is_elevated(pid) || is_elevated(owner) {
            return Ok(None);
        }
        let app = process_name(owner);
        let is_password = element
            .CurrentIsPassword()
            .map(|b| b.as_bool())
            .unwrap_or(true);
        let Ok(caret) = caret_range(&element) else {
            return Ok(None);
        };
        let (before, after) = if is_password || config.is_blocked(&app) {
            (String::new(), String::new())
        } else {
            let lookback = config.lookback_chars();
            text_around(&caret, i32::try_from(lookback).unwrap_or(i32::MAX), 200)?
        };
        let (font_family, font_points, text_color) = font(&caret);
        let dpi = GetDpiForWindow(root).max(96) as f64;
        Ok(Some(FieldSnapshot {
            field_id: runtime_id(&element),
            app,
            window_title: window_title(root),
            before: normalize(before),
            after: normalize(after),
            has_selection: has_selection(&element),
            is_password,
            composing: composing(&element),
            caret: caret_rect(&caret, foreground),
            font_family,
            font_px: font_points.map(|pt| (pt * dpi / 72.0) as f32),
            text_color,
        }))
    }
}
