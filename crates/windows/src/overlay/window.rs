use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, HWND_TOPMOST, MA_NOACTIVATE, RegisterClassW, SW_HIDE,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, ShowWindow, ULW_ALPHA,
    UpdateLayeredWindow, WM_MOUSEACTIVATE, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use super::fade;
use super::raster::Raster;
use super::screen::ScreenDc;

pub const OVERLAY_CLASS: PCWSTR = w!("PithGhostText");
const ORIGIN: POINT = POINT { x: 0, y: 0 };

unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_TIMER if wparam.0 == fade::TIMER => {
            fade::advance(hwnd);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

pub struct Layered {
    hwnd: HWND,
    visible: bool,
}

impl Layered {
    pub fn create() -> windows::core::Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: OVERLAY_CLASS,
                ..Default::default()
            };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW,
                OVERLAY_CLASS,
                w!("pith"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            Ok(Self {
                hwnd,
                visible: false,
            })
        }
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn present(
        &mut self,
        screen: &ScreenDc,
        raster: &Raster,
        destination: POINT,
        animated: bool,
    ) {
        if !self.visible {
            fade::reset(self.hwnd, animated);
        }
        unsafe {
            let _ = UpdateLayeredWindow(
                self.hwnd,
                Some(screen.0),
                Some(&destination),
                Some(&raster.size),
                Some(raster.source()),
                Some(&ORIGIN),
                COLORREF(0),
                Some(&fade::blend(fade::step(self.hwnd))),
                ULW_ALPHA,
            );
        }
        self.reveal();
    }

    pub fn hide(&mut self) {
        if !self.visible {
            return;
        }
        fade::stop(self.hwnd);
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        self.visible = false;
    }

    fn reveal(&mut self) {
        if !self.visible {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
            fade::start(self.hwnd);
            self.visible = true;
        }
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}
