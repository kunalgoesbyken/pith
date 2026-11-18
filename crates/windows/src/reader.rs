use std::sync::mpsc::{Receiver, Sender, channel};

use nib_core::FieldSnapshot;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Focused,
    Window(isize),
}

pub type SnapshotCallback = Box<dyn Fn(Option<FieldSnapshot>) + Send>;

pub struct Reader {
    pokes: Sender<()>,
}

impl Reader {
    pub fn spawn(source: Source, context_chars: usize, on_snapshot: SnapshotCallback) -> Self {
        let (pokes, rx) = channel();
        std::thread::Builder::new()
            .name("nib-uia".into())
            .spawn(move || serve(source, context_chars, rx, on_snapshot))
            .expect("spawn uia thread");
        Self { pokes }
    }

    pub fn poke(&self) {
        let _ = self.pokes.send(());
    }

    pub fn sender(&self) -> Sender<()> {
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

fn serve(source: Source, context_chars: usize, rx: Receiver<()>, on_snapshot: SnapshotCallback) {
    let automation = match automation() {
        Ok(automation) => automation,
        Err(error) => return nib_core::log(&format!("uia unavailable: {error}")),
    };
    let own_pid = std::process::id();
    let mut last: Option<FieldSnapshot> = None;
    let mut first = true;
    while rx.recv().is_ok() {
        while rx.try_recv().is_ok() {}
        let snapshot = read(&automation, source, context_chars, own_pid)
            .ok()
            .flatten();
        if first || snapshot != last {
            first = false;
            last = snapshot.clone();
            on_snapshot(snapshot);
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
    context_chars: usize,
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
                let hwnd = HWND(handle as _);
                (
                    automation.ElementFromHandle(hwnd)?,
                    GetAncestor(hwnd, GA_ROOT),
                    None,
                )
            }
        };
        let pid = element.CurrentProcessId()? as u32;
        if source == Source::Focused && pid == own_pid {
            return Ok(None);
        }
        let owner = window_pid(root).unwrap_or(pid);
        if is_elevated(pid) || is_elevated(owner) {
            return Ok(None);
        }
        let Ok(caret) = caret_range(&element) else {
            return Ok(None);
        };
        let (before, after) = text_around(&caret, context_chars as i32, 200)?;
        let (font_family, font_points, text_color) = font(&caret);
        let dpi = GetDpiForWindow(root).max(96) as f64;
        Ok(Some(FieldSnapshot {
            field_id: runtime_id(&element),
            app: process_name(owner),
            window_title: window_title(root),
            before: normalize(before),
            after: normalize(after),
            has_selection: has_selection(&element),
            is_password: element
                .CurrentIsPassword()
                .map(|b| b.as_bool())
                .unwrap_or(false),
            composing: composing(&element),
            caret: caret_rect(&caret, foreground),
            font_family,
            font_px: font_points.map(|pt| (pt * dpi / 72.0) as f32),
            text_color,
        }))
    }
}
