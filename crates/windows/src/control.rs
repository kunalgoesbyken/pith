use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use crate::appearance::is_appearance_change;

const WM_WAKE: u32 = WM_APP + 1;

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

    pub fn attach(&self, hwnd: HWND) {
        self.0.store(hwnd.0 as isize, Ordering::SeqCst);
    }
}

pub enum Signal {
    Wake,
    Timer(usize),
    Hotkey(i32),
    Power,
    Appearance,
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
        WM_POWERBROADCAST if wparam.0 as u32 == PBT_POWERSETTINGCHANGE => Some(Signal::Power),
        _ if is_appearance_change(message, wparam, lparam) => Some(Signal::Appearance),
        _ => None,
    };
    if let Some(signal) = signal {
        let handled = LRESULT(isize::from(matches!(signal, Signal::Power)));
        SIGNALS.with(|queue| queue.borrow_mut().push_back(signal));
        return handled;
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

pub fn control_window() -> windows::core::Result<HWND> {
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
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }
}

pub fn next_signal() -> Option<Signal> {
    SIGNALS.with(|queue| queue.borrow_mut().pop_front())
}
