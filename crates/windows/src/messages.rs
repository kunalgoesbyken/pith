use windows::Win32::UI::WindowsAndMessaging::WM_APP;

pub const WM_WAKE: u32 = WM_APP + 1;
pub const WM_RECORDED: u32 = WM_APP + 2;
pub const WM_RELAYOUT: u32 = WM_APP + 3;
pub const WM_SLIDE_TO: u32 = WM_APP + 4;
