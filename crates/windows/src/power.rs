use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::System::Power::{
    GetSystemPowerStatus, HPOWERNOTIFY, RegisterPowerSettingNotification, SYSTEM_POWER_STATUS,
};
use windows::Win32::System::SystemServices::GUID_POWER_SAVING_STATUS;
use windows::Win32::UI::WindowsAndMessaging::DEVICE_NOTIFY_WINDOW_HANDLE;
use windows::core::Owned;

const NO_OR_UNKNOWN_BATTERY: u8 = 128;
const BATTERY_SAVER_ON: u8 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Power {
    #[default]
    Unrestricted,
    BatterySaver,
}

impl Power {
    fn of(status: &SYSTEM_POWER_STATUS) -> Self {
        let has_battery = status.BatteryFlag & NO_OR_UNKNOWN_BATTERY == 0;
        if has_battery && status.SystemStatusFlag == BATTERY_SAVER_ON {
            Self::BatterySaver
        } else {
            Self::Unrestricted
        }
    }

    pub fn read() -> Self {
        let mut status = SYSTEM_POWER_STATUS::default();
        match unsafe { GetSystemPowerStatus(&mut status) } {
            Ok(()) => Self::of(&status),
            Err(_) => Self::Unrestricted,
        }
    }
}

pub fn watch(window: HWND) -> Option<Owned<HPOWERNOTIFY>> {
    let handle = unsafe {
        RegisterPowerSettingNotification(
            HANDLE(window.0),
            &GUID_POWER_SAVING_STATUS,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        )
    }
    .ok()?;
    Some(unsafe { Owned::new(handle) })
}
