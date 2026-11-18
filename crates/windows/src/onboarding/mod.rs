mod layout;
mod model;
mod step;
mod view;
mod window;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};

use pith_core::{Reach, log};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Controls::EM_SETSEL;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, SW_HIDE, SW_SHOWNORMAL, SendMessageW, SetWindowTextW, ShowWindow, WM_SETTEXT,
};
use windows::core::PCWSTR;

use crate::reach;
use crate::util::wide;

pub use step::{Event, Step, View};

const PRACTICE_START: &str =
    "Thank you for getting back to me so quickly. I have read the proposal and";
const MARKER_CONTENT: &str = "done";

const DONE_ID: usize = 1001;
const STATUS_ID: usize = 1002;
const PROGRESS_ID: usize = 1003;
const HINT_ID: usize = 1004;
const PRACTICE_ID: usize = 1005;
const KEYS_ID: usize = 1006;

static PRACTICE: AtomicIsize = AtomicIsize::new(0);

pub fn is_practice(hwnd: HWND) -> bool {
    hwnd.0 as isize == PRACTICE.load(Ordering::SeqCst) && !hwnd.0.is_null()
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

pub struct Onboarding {
    controls: Controls,
    step: Step,
    marker: PathBuf,
    visible: bool,
}

impl Onboarding {
    pub fn create(marker: PathBuf, visible: bool, reach: &Reach) -> windows::core::Result<Self> {
        let controls = window::create(&Step::Preparing.view(), reach::promise(reach))?;
        if visible {
            unsafe {
                let _ = ShowWindow(controls.window, SW_SHOWNORMAL);
            }
        }
        let mut onboarding = Self {
            controls,
            step: Step::Preparing,
            marker,
            visible,
        };
        onboarding.render(&Step::Preparing);
        Ok(onboarding)
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
        if let Some(command) = window::take_command() {
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
        model::update(|model| model.view = view.clone());
        window::relayout(controls.window);
        unsafe {
            let _ = SetWindowTextW(controls.status, PCWSTR(status.as_ptr()));
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
        }
        window::release_font();
    }
}

#[cfg(test)]
mod tests {
    use super::layout::build;
    use super::model::Model;
    use super::*;
    use crate::ui::{self, Scale, Theme, force_dark_theme};
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, WM_DPICHANGED, WM_SETTINGCHANGE};
    use windows::core::w;

    fn marker(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pith-onboarding-{name}-{}", std::process::id()))
    }

    fn frame_width(hwnd: HWND) -> i32 {
        let mut frame = RECT::default();
        unsafe {
            let _ = GetWindowRect(hwnd, &mut frame);
        }
        ui::width(frame)
    }

    #[test]
    fn dpi_and_theme_changes_are_applied_live() {
        let window = Onboarding::create(marker("live"), false, &Reach::ThisPc).unwrap();
        let hwnd = window.controls().window;
        let before = frame_width(hwnd);
        let suggested = RECT::default();
        unsafe {
            SendMessageW(
                hwnd,
                WM_DPICHANGED,
                Some(WPARAM(192 | (192 << 16))),
                Some(LPARAM(&suggested as *const RECT as isize)),
            );
        }
        assert!(frame_width(hwnd) > before * 3 / 2);
        force_dark_theme(Some(true));
        unsafe {
            SendMessageW(
                hwnd,
                WM_SETTINGCHANGE,
                Some(WPARAM(0)),
                Some(LPARAM(w!("ImmersiveColorSet").as_ptr() as isize)),
            );
        }
        assert_eq!(model::current().unwrap().theme, Theme::detect());
        force_dark_theme(None);
    }

    #[test]
    fn the_layout_stacks_without_overlap_at_every_scale() {
        for dpi in [96, 120, 144, 192] {
            let model = Model {
                theme: Theme::detect(),
                mica: false,
                scale: Scale(dpi),
                view: Step::Failed("a long failure message ".repeat(8)).view(),
                promise: reach::promise(&Reach::ThisPc),
            };
            let layout = build(&model);
            let stack = [
                layout.title,
                layout.promise,
                layout.status_card,
                layout.keys_card,
                layout.note,
                layout.practice_card,
                layout.footer,
            ];
            for pair in stack.windows(2) {
                assert!(pair[0].bottom <= pair[1].top, "{dpi}");
            }
            assert_eq!(layout.client.bottom, layout.footer.bottom);
            assert!(layout.practice.bottom <= layout.practice_card.bottom);
        }
    }
}
