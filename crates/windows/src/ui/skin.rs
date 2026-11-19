use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, HDC, PAINTSTRUCT};
use windows::Win32::UI::Controls::{
    DRAWITEMSTRUCT, ODA_DRAWENTIRE, ODS_DISABLED, ODS_FOCUS, ODS_NOFOCUSRECT, ODS_SELECTED,
    ODT_BUTTON,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, IsWindowEnabled};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    BM_GETSTATE, BST_PUSHED, GetClientRect, GetDlgCtrlID, GetParent, SendMessageW, UISF_HIDEFOCUS,
    WM_DRAWITEM, WM_ERASEBKGND, WM_NCDESTROY, WM_PAINT, WM_PRINTCLIENT, WM_QUERYUISTATE,
};

const SUBCLASS_ID: usize = 0x534B_494E;

fn focus_cue_hidden(hwnd: HWND) -> bool {
    let state = unsafe { SendMessageW(hwnd, WM_QUERYUISTATE, None, None) };
    state.0 as u32 & UISF_HIDEFOCUS != 0
}

fn item_state(hwnd: HWND, pressed: bool) -> u32 {
    let mut flags = 0;
    if pressed {
        flags |= ODS_SELECTED.0;
    }
    if unsafe { GetFocus() } == hwnd {
        flags |= ODS_FOCUS.0;
    }
    if !unsafe { IsWindowEnabled(hwnd) }.as_bool() {
        flags |= ODS_DISABLED.0;
    }
    if focus_cue_hidden(hwnd) {
        flags |= ODS_NOFOCUSRECT.0;
    }
    flags
}

pub fn request_draw(hwnd: HWND, hdc: HDC, pressed: bool) {
    let mut bounds = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut bounds);
    }
    let id = unsafe { GetDlgCtrlID(hwnd) };
    let item = DRAWITEMSTRUCT {
        CtlType: ODT_BUTTON,
        CtlID: id as u32,
        itemAction: ODA_DRAWENTIRE,
        itemState: windows::Win32::UI::Controls::ODS_FLAGS(item_state(hwnd, pressed)),
        hwndItem: hwnd,
        hDC: hdc,
        rcItem: bounds,
        ..Default::default()
    };
    if let Ok(parent) = unsafe { GetParent(hwnd) } {
        unsafe {
            SendMessageW(
                parent,
                WM_DRAWITEM,
                Some(WPARAM(id as usize)),
                Some(LPARAM(&item as *const DRAWITEMSTRUCT as isize)),
            );
        }
    }
}

fn is_pushed(hwnd: HWND) -> bool {
    let state = unsafe { SendMessageW(hwnd, BM_GETSTATE, None, None) };
    state.0 as u32 & BST_PUSHED != 0
}

fn paint(hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
    request_draw(hwnd, hdc, is_pushed(hwnd));
    unsafe {
        let _ = EndPaint(hwnd, &paint);
    }
}

unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match message {
        WM_PAINT => {
            paint(hwnd);
            return LRESULT(0);
        }
        WM_PRINTCLIENT => {
            request_draw(hwnd, HDC(wparam.0 as _), is_pushed(hwnd));
            return LRESULT(0);
        }
        WM_ERASEBKGND => return LRESULT(1),
        WM_NCDESTROY => unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(procedure), SUBCLASS_ID);
        },
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

pub fn attach(hwnd: HWND) {
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(procedure), SUBCLASS_ID, 0);
    }
}
