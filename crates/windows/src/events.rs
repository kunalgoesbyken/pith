use std::sync::Mutex;
use std::sync::mpsc::Sender;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::reader::Poke;

static TARGET: Mutex<Option<Sender<Poke>>> = Mutex::new(None);

const EVENTS: [u32; 5] = [
    EVENT_OBJECT_FOCUS,
    EVENT_OBJECT_VALUECHANGE,
    EVENT_OBJECT_TEXTSELECTIONCHANGED,
    EVENT_OBJECT_LOCATIONCHANGE,
    EVENT_SYSTEM_FOREGROUND,
];

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    if event == EVENT_OBJECT_LOCATIONCHANGE && object != OBJID_CARET.0 {
        return;
    }
    let follows_focus = event == EVENT_OBJECT_FOCUS || event == EVENT_SYSTEM_FOREGROUND;
    if !follows_focus
        && !hwnd.is_invalid()
        && unsafe { GetAncestor(hwnd, GA_ROOT) != GetForegroundWindow() }
    {
        return;
    }
    if let Ok(guard) = TARGET.lock()
        && let Some(sender) = guard.as_ref()
    {
        let _ = sender.send(Poke::Changed);
    }
}

pub struct Hooks(Vec<HWINEVENTHOOK>);

impl Hooks {
    pub fn install(target: Sender<Poke>) -> Self {
        if let Ok(mut guard) = TARGET.lock() {
            *guard = Some(target);
        }
        let hooks = EVENTS
            .iter()
            .map(|event| unsafe {
                SetWinEventHook(
                    *event,
                    *event,
                    None,
                    Some(on_event),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                )
            })
            .filter(|hook| !hook.is_invalid())
            .collect();
        Self(hooks)
    }
}

impl Drop for Hooks {
    fn drop(&mut self) {
        for hook in &self.0 {
            unsafe {
                let _ = UnhookWinEvent(*hook);
            }
        }
    }
}
