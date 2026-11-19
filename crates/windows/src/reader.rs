use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use pith_core::{
    AFTER_CHARS, Config, FieldParts, FieldSnapshot, NearbyCache, SelectOutcome, Surrounding,
    is_redacted, place,
};
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
    caret_range, caret_rect, collapse_selection, composing, font, has_selection, runtime_id,
    select_before_caret, text_around,
};
use crate::language::input_language;
use crate::nearby;
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

pub struct Selection {
    pub field_id: u64,
    pub text: String,
    pub done: Box<dyn FnOnce(SelectOutcome) + Send>,
}

pub enum Poke {
    Changed,
    Key(u64),
    Select(Selection),
    Collapse(u64, String),
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

    pub fn select(&self, selection: Selection) {
        let _ = self.pokes.send(Poke::Select(selection));
    }

    pub fn collapse(&self, field_id: u64, text: String) {
        let _ = self.pokes.send(Poke::Collapse(field_id, text));
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
    let mut nearby = NearbyCache::default();
    let mut first = true;
    let mut errors = 0;
    while let Ok(request) = rx.recv() {
        let mut served = None;
        for poke in std::iter::once(request).chain(rx.try_iter()) {
            match poke {
                Poke::Changed => {}
                Poke::Key(id) => served = served.max(Some(id)),
                Poke::Select(selection) => select(&automation, source, selection),
                Poke::Collapse(field_id, text) => collapse(&automation, source, field_id, &text),
            }
        }
        let current = config
            .lock()
            .map(|shared| shared.clone())
            .unwrap_or_default();
        let outcome = if current.enabled {
            read(&automation, source, &current, own_pid, &mut nearby)
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

struct Target {
    element: IUIAutomationElement,
    root: HWND,
    foreground: Option<HWND>,
}

fn target(automation: &IUIAutomation, source: Source) -> Result<Target> {
    unsafe {
        Ok(match source {
            Source::Focused => {
                let foreground = GetForegroundWindow();
                Target {
                    element: automation.GetFocusedElement()?,
                    root: foreground,
                    foreground: Some(foreground),
                }
            }
            Source::Window(handle) => {
                let hwnd = HWND(handle.load(Ordering::SeqCst) as _);
                Target {
                    element: automation.ElementFromHandle(hwnd)?,
                    root: GetAncestor(hwnd, GA_ROOT),
                    foreground: None,
                }
            }
        })
    }
}

fn field_target(automation: &IUIAutomation, source: Source, field_id: u64) -> Option<Target> {
    target(automation, source)
        .ok()
        .filter(|target| runtime_id(&target.element) == field_id)
}

fn select(automation: &IUIAutomation, source: Source, selection: Selection) {
    let Selection {
        field_id,
        text,
        done,
    } = selection;
    let outcome = field_target(automation, source, field_id)
        .map_or(SelectOutcome::Lost, |target| {
            select_before_caret(&target.element, &text)
        });
    done(outcome);
}

fn collapse(automation: &IUIAutomation, source: Source, field_id: u64, text: &str) {
    if let Some(target) = field_target(automation, source, field_id) {
        let _ = collapse_selection(&target.element, text);
    }
}

fn read(
    automation: &IUIAutomation,
    source: Source,
    config: &Config,
    own_pid: u32,
    nearby: &mut NearbyCache,
) -> Result<Option<FieldSnapshot>> {
    unsafe {
        let Target {
            element,
            root,
            foreground,
        } = target(automation, source)?;
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
        let is_password = element
            .CurrentIsPassword()
            .map(|b| b.as_bool())
            .unwrap_or(true);
        let Ok(caret) = caret_range(&element) else {
            return Ok(None);
        };
        let redacted = is_redacted(config, app.as_deref(), is_password);
        let surrounding = if redacted {
            None
        } else {
            let lookback = config.lookback_chars();
            let (before, after) = text_around(
                &caret,
                i32::try_from(lookback).unwrap_or(i32::MAX),
                i32::try_from(AFTER_CHARS).unwrap_or(i32::MAX),
            )?;
            Some(Surrounding { before, after })
        };
        let title = window_title(root);
        let own_window = matches!(source, Source::Focused) && pid == own_pid;
        let cues = if redacted || own_window || pid != owner || !config.uses_nearby() {
            nearby.clear();
            Vec::new()
        } else {
            let before_empty = surrounding
                .as_ref()
                .is_none_or(|text| text.before.trim().is_empty());
            let key = (runtime_id(&element), title.clone());
            nearby
                .fresh(&key, before_empty)
                .unwrap_or_else(|| nearby.store(key, nearby::read(automation, &element, root)))
        };
        let style = font(&caret, GetDpiForWindow(root).max(96));
        let element_rect = element.CurrentBoundingRectangle().ok().map(rect_of);
        Ok(Some(FieldSnapshot::from_parts(
            config,
            FieldParts {
                field_id: runtime_id(&element),
                app,
                window_title: title,
                surrounding,
                has_selection: has_selection(&element),
                is_password,
                composing: composing(&element),
                language: input_language(root),
                placement: place(caret_rect(&caret, foreground), element_rect, &work_areas()),
                style,
                nearby: cues,
            },
        )))
    }
}
