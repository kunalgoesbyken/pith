use std::cell::Cell;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};

use pith_core::log;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    COLOR_WINDOW, CreateFontIndirectW, DeleteObject, HBRUSH, HFONT,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::Controls::{
    EM_SETSEL, ICC_PROGRESS_CLASS, INITCOMMONCONTROLSEX, InitCommonControlsEx, PBM_SETPOS,
    PBM_SETRANGE32, PROGRESS_CLASSW,
};
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, GetDpiForSystem, SystemParametersInfoForDpi,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use crate::util::wide;

const PRACTICE_START: &str =
    "Thank you for getting back to me so quickly. I have read the proposal and";
const KEYS: &str = "Tab keeps the suggestion.\r\nShift+Tab keeps one word.\r\nEsc hides it.\r\nWith no suggestion showing, Tab is a normal Tab.";
const PRACTICE_HINT: &str = "Try it. Click at the end of the sentence below and pause for a second. The rest of the sentence appears in grey.";
const MARKER_CONTENT: &str = "done";

const DONE_ID: usize = 1001;
const WIDTH: i32 = 500;
const HEIGHT: i32 = 376;
const MARGIN: i32 = 20;
const MEGABYTE: u64 = 1_000_000;

static PRACTICE: AtomicIsize = AtomicIsize::new(0);

pub fn is_practice(hwnd: HWND) -> bool {
    hwnd.0 as isize == PRACTICE.load(Ordering::SeqCst) && !hwnd.0.is_null()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Preparing,
    Downloading { done: u64, total: u64 },
    Loading,
    Practice,
    Failed(String),
    Finished,
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Progress(u64, u64),
    Loading,
    Ready,
    Failed(String),
    Done,
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub status: String,
    pub permille: Option<u32>,
    pub practice_enabled: bool,
    pub done_enabled: bool,
}

fn megabytes(bytes: u64) -> u64 {
    bytes.saturating_add(MEGABYTE / 2) / MEGABYTE
}

impl Step {
    pub fn next(self, event: Event) -> Step {
        match (self, event) {
            (step @ (Step::Finished | Step::Closed), _) => step,
            (_, Event::Close) => Step::Closed,
            (Step::Practice, Event::Done) => Step::Finished,
            (Step::Failed(_), Event::Done) => Step::Closed,
            (step @ Step::Practice, _) | (step @ Step::Failed(_), _) => step,
            (_, Event::Ready) => Step::Practice,
            (_, Event::Failed(error)) => Step::Failed(error),
            (_, Event::Progress(done, total)) if done >= total && total > 0 => Step::Loading,
            (_, Event::Progress(done, total)) => Step::Downloading {
                done: done.min(total),
                total,
            },
            (Step::Preparing, Event::Loading) => Step::Loading,
            (step, _) => step,
        }
    }

    pub fn is_over(&self) -> bool {
        matches!(self, Step::Finished | Step::Closed)
    }

    pub fn view(&self) -> View {
        let (status, permille) = match self {
            Step::Preparing => ("Starting Pith".to_string(), None),
            Step::Downloading { done, total } => (
                format!(
                    "Downloading the model: {} MB of {} MB",
                    megabytes(*done),
                    megabytes(*total)
                ),
                Some(((*done as u128 * 1000) / (*total).max(1) as u128) as u32),
            ),
            Step::Loading => ("Loading the model".to_string(), None),
            Step::Practice | Step::Finished | Step::Closed => ("Ready".to_string(), None),
            Step::Failed(error) => (format!("Pith could not start: {error}"), None),
        };
        View {
            status,
            permille,
            practice_enabled: matches!(self, Step::Practice),
            done_enabled: matches!(self, Step::Practice | Step::Failed(_)),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Controls {
    pub window: HWND,
    pub status: HWND,
    pub progress: HWND,
    pub hint: HWND,
    pub practice: HWND,
    pub keys: HWND,
    pub done: HWND,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pending,
    Finished,
    Closed,
}

thread_local! {
    static COMMAND: Cell<Option<Event>> = const { Cell::new(None) };
}

unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let command = match message {
        WM_COMMAND if wparam.0 & 0xFFFF == DONE_ID => Some(Event::Done),
        WM_CLOSE => Some(Event::Close),
        _ => None,
    };
    if let Some(command) = command {
        COMMAND.with(|cell| cell.set(Some(command)));
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn scaled(pixels: i32, dpi: u32) -> i32 {
    pixels * dpi as i32 / 96
}

fn message_font(dpi: u32) -> Option<HFONT> {
    unsafe {
        let mut metrics = NONCLIENTMETRICSW {
            cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
            ..Default::default()
        };
        SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            metrics.cbSize,
            Some(&mut metrics as *mut _ as *mut _),
            0,
            dpi,
        )
        .ok()?;
        let font = CreateFontIndirectW(&metrics.lfMessageFont);
        (!font.is_invalid()).then_some(font)
    }
}

struct Child<'a> {
    class: PCWSTR,
    text: &'a str,
    bounds: (i32, i32, i32, i32),
    style: u32,
    extended: WINDOW_EX_STYLE,
    id: usize,
}

impl<'a> Child<'a> {
    fn new(class: PCWSTR, text: &'a str, bounds: (i32, i32, i32, i32)) -> Self {
        Self {
            class,
            text,
            bounds,
            style: 0,
            extended: WINDOW_EX_STYLE(0),
            id: 0,
        }
    }
}

struct Layout {
    dpi: u32,
}

impl Layout {
    fn add(&self, parent: HWND, child: Child) -> windows::core::Result<HWND> {
        let text = wide(child.text);
        let bounds = child.bounds;
        unsafe {
            let instance = GetModuleHandleW(None)?;
            CreateWindowExW(
                child.extended,
                child.class,
                PCWSTR(text.as_ptr()),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(child.style),
                scaled(bounds.0, self.dpi),
                scaled(bounds.1, self.dpi),
                scaled(bounds.2, self.dpi),
                scaled(bounds.3, self.dpi),
                Some(parent),
                Some(HMENU(child.id as _)),
                Some(instance.into()),
                None,
            )
        }
    }
}

pub struct Onboarding {
    controls: Controls,
    font: Option<HFONT>,
    step: Step,
    marker: PathBuf,
    visible: bool,
}

impl Onboarding {
    pub fn create(marker: PathBuf, visible: bool) -> windows::core::Result<Self> {
        unsafe {
            let _ = LoadLibraryW(w!("Msftedit.dll"));
            let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
                dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_PROGRESS_CLASS,
            });
            let instance = GetModuleHandleW(None)?;
            let class = w!("PithOnboarding");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: class,
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as _),
                ..Default::default()
            };
            RegisterClassW(&wc);
            let dpi = GetDpiForSystem();
            let style = WS_CAPTION | WS_SYSMENU;
            let mut frame = RECT {
                left: 0,
                top: 0,
                right: scaled(WIDTH, dpi),
                bottom: scaled(HEIGHT, dpi),
            };
            AdjustWindowRectExForDpi(&mut frame, style, false, WINDOW_EX_STYLE(0), dpi)?;
            let width = frame.right - frame.left;
            let height = frame.bottom - frame.top;
            let window = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("Welcome to Pith"),
                style,
                (GetSystemMetrics(SM_CXSCREEN) - width) / 2,
                (GetSystemMetrics(SM_CYSCREEN) - height) / 3,
                width,
                height,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            let layout = Layout { dpi };
            let inner = WIDTH - 2 * MARGIN;
            let label = w!("STATIC");
            let status = layout.add(window, Child::new(label, "", (MARGIN, 16, inner, 20)))?;
            let progress = layout.add(
                window,
                Child::new(PROGRESS_CLASSW, "", (MARGIN, 42, inner, 16)),
            )?;
            let hint = layout.add(
                window,
                Child::new(label, PRACTICE_HINT, (MARGIN, 76, inner, 36)),
            )?;
            let practice = layout.add(
                window,
                Child {
                    style: (ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32 | WS_VSCROLL.0,
                    extended: WS_EX_CLIENTEDGE,
                    ..Child::new(w!("RICHEDIT50W"), "", (MARGIN, 116, inner, 110))
                },
            )?;
            let keys = layout.add(window, Child::new(label, KEYS, (MARGIN, 240, inner, 80)))?;
            let done = layout.add(
                window,
                Child {
                    style: (BS_DEFPUSHBUTTON as u32) | WS_TABSTOP.0,
                    id: DONE_ID,
                    ..Child::new(
                        w!("BUTTON"),
                        "Done",
                        (WIDTH - MARGIN - 96, HEIGHT - 16 - 30, 96, 30),
                    )
                },
            )?;
            SendMessageW(
                progress,
                PBM_SETRANGE32,
                Some(WPARAM(0)),
                Some(LPARAM(1000)),
            );
            let font = message_font(dpi);
            if let Some(font) = font {
                for control in [status, hint, practice, keys, done] {
                    SendMessageW(
                        control,
                        WM_SETFONT,
                        Some(WPARAM(font.0 as usize)),
                        Some(LPARAM(1)),
                    );
                }
            }
            if visible {
                let _ = ShowWindow(window, SW_SHOWNORMAL);
            }
            let mut onboarding = Self {
                controls: Controls {
                    window,
                    status,
                    progress,
                    hint,
                    practice,
                    keys,
                    done,
                },
                font,
                step: Step::Preparing,
                marker,
                visible,
            };
            onboarding.render(&Step::Preparing);
            Ok(onboarding)
        }
    }

    pub fn controls(&self) -> Controls {
        self.controls
    }

    pub fn step(&self) -> &Step {
        &self.step
    }

    pub fn event(&mut self, event: Event) {
        let previous = std::mem::replace(&mut self.step, Step::Preparing);
        self.step = previous.clone().next(event);
        self.render(&previous);
    }

    pub fn poll(&mut self) -> Outcome {
        if let Some(command) = COMMAND.with(Cell::take) {
            self.event(command);
        }
        match self.step {
            Step::Finished => {
                self.write_marker();
                Outcome::Finished
            }
            Step::Closed => Outcome::Closed,
            _ => Outcome::Pending,
        }
    }

    fn write_marker(&self) {
        if let Some(parent) = self.marker.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(error) = fs::write(&self.marker, MARKER_CONTENT) {
            log(&format!("could not save onboarding state: {error}"));
        }
    }

    fn render(&mut self, previous: &Step) {
        let view = self.step.view();
        let controls = self.controls;
        let status = wide(&view.status);
        unsafe {
            let _ = SetWindowTextW(controls.status, PCWSTR(status.as_ptr()));
            let progress_visible = if view.permille.is_some() {
                SW_SHOW
            } else {
                SW_HIDE
            };
            let _ = ShowWindow(controls.progress, progress_visible);
            if let Some(permille) = view.permille {
                SendMessageW(
                    controls.progress,
                    PBM_SETPOS,
                    Some(WPARAM(permille as usize)),
                    Some(LPARAM(0)),
                );
            }
            let practice_visible = if view.practice_enabled {
                SW_SHOW
            } else {
                SW_HIDE
            };
            for control in [controls.hint, controls.practice, controls.keys] {
                let _ = ShowWindow(control, practice_visible);
            }
            let _ = EnableWindow(controls.practice, view.practice_enabled);
            let _ = EnableWindow(controls.done, view.done_enabled);
            if self.step == Step::Practice && *previous != Step::Practice {
                self.start_practice();
            }
            if self.step.is_over() {
                PRACTICE.store(0, Ordering::SeqCst);
                let _ = ShowWindow(controls.window, SW_HIDE);
            }
        }
    }

    fn start_practice(&self) {
        let text = wide(PRACTICE_START);
        let end = PRACTICE_START.encode_utf16().count();
        unsafe {
            SendMessageW(
                self.controls.practice,
                WM_SETTEXT,
                Some(WPARAM(0)),
                Some(LPARAM(text.as_ptr() as isize)),
            );
            SendMessageW(
                self.controls.practice,
                EM_SETSEL,
                Some(WPARAM(end)),
                Some(LPARAM(end as isize)),
            );
            PRACTICE.store(self.controls.practice.0 as isize, Ordering::SeqCst);
            if self.visible {
                let _ = SetFocus(Some(self.controls.practice));
            }
        }
    }
}

impl Drop for Onboarding {
    fn drop(&mut self) {
        PRACTICE.store(0, Ordering::SeqCst);
        unsafe {
            let _ = DestroyWindow(self.controls.window);
            if let Some(font) = self.font {
                let _ = DeleteObject(font.into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn downloading(done: u64, total: u64) -> Step {
        Step::Downloading { done, total }
    }

    #[test]
    fn walks_download_loading_ready_done() {
        let mut step = Step::Preparing;
        for (event, expected) in [
            (Event::Progress(0, 397), downloading(0, 397)),
            (Event::Progress(100, 397), downloading(100, 397)),
            (Event::Loading, downloading(100, 397)),
            (Event::Progress(397, 397), Step::Loading),
            (Event::Loading, Step::Loading),
            (Event::Ready, Step::Practice),
            (Event::Done, Step::Finished),
        ] {
            step = step.next(event);
            assert_eq!(step, expected);
        }
    }

    #[test]
    fn cached_model_skips_the_download() {
        let step = Step::Preparing.next(Event::Loading).next(Event::Ready);
        assert_eq!(step, Step::Practice);
    }

    #[test]
    fn remote_backend_goes_straight_to_practice() {
        assert_eq!(Step::Preparing.next(Event::Ready), Step::Practice);
    }

    #[test]
    fn done_is_ignored_until_practice() {
        for step in [Step::Preparing, downloading(1, 2), Step::Loading] {
            assert_eq!(step.clone().next(Event::Done), step);
        }
        assert_eq!(Step::Failed("x".into()).next(Event::Done), Step::Closed);
    }

    #[test]
    fn practice_ignores_late_download_events() {
        let step = Step::Practice;
        for event in [
            Event::Progress(1, 2),
            Event::Loading,
            Event::Failed("late".into()),
        ] {
            assert_eq!(step.clone().next(event), Step::Practice);
        }
    }

    #[test]
    fn failure_is_final_until_closed() {
        let failed = Step::Preparing.next(Event::Failed("offline".into()));
        assert_eq!(failed, Step::Failed("offline".into()));
        assert_eq!(failed.clone().next(Event::Ready), failed);
        assert_eq!(failed.next(Event::Close), Step::Closed);
    }

    #[test]
    fn closing_works_from_every_live_step() {
        for step in [
            Step::Preparing,
            downloading(1, 2),
            Step::Loading,
            Step::Practice,
            Step::Failed("x".into()),
        ] {
            assert_eq!(step.next(Event::Close), Step::Closed);
        }
    }

    #[test]
    fn finished_and_closed_are_terminal() {
        for end in [Step::Finished, Step::Closed] {
            for event in [
                Event::Ready,
                Event::Done,
                Event::Loading,
                Event::Close,
                Event::Progress(1, 2),
                Event::Failed("x".into()),
            ] {
                assert_eq!(end.clone().next(event), end);
            }
        }
    }

    #[test]
    fn progress_is_clamped_and_zero_total_never_completes() {
        assert_eq!(
            Step::Preparing.next(Event::Progress(0, 0)),
            downloading(0, 0)
        );
        assert_eq!(downloading(0, 0).view().permille, Some(0));
        assert_eq!(Step::Preparing.next(Event::Progress(9, 5)), Step::Loading);
    }

    #[test]
    fn view_reports_size_and_gates_controls() {
        let view = downloading(396_704_960 / 2, 396_704_960).view();
        assert_eq!(view.status, "Downloading the model: 198 MB of 397 MB");
        assert_eq!(view.permille, Some(500));
        assert!(!view.practice_enabled && !view.done_enabled);
        let practice = Step::Practice.view();
        assert_eq!(practice.status, "Ready");
        assert!(practice.practice_enabled && practice.done_enabled);
        let failed = Step::Failed("offline".into()).view();
        assert_eq!(failed.status, "Pith could not start: offline");
        assert!(!failed.practice_enabled && failed.done_enabled);
    }

    #[test]
    fn view_never_overflows_on_huge_sizes() {
        let view = downloading(u64::MAX - 1, u64::MAX).view();
        assert_eq!(view.permille, Some(999));
    }
}
