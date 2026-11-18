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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registering_on_the_message_only_control_window_delivers_a_power_signal() {
        use crate::control::{Signal, control_window, next_signal};
        use std::time::{Duration, Instant};
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
        };
        let window = control_window().unwrap();
        let _watch = watch(window).expect("registered");
        let started = Instant::now();
        let mut delivered = false;
        while !delivered && started.elapsed() < Duration::from_secs(3) {
            let mut message = MSG::default();
            unsafe {
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            while let Some(signal) = next_signal() {
                delivered |= matches!(signal, Signal::Power);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(delivered);
    }

    fn status(battery: u8, saver: u8) -> SYSTEM_POWER_STATUS {
        SYSTEM_POWER_STATUS {
            BatteryFlag: battery,
            SystemStatusFlag: saver,
            ..Default::default()
        }
    }

    #[test]
    fn only_a_machine_with_a_battery_and_battery_saver_on_is_restricted() {
        let rows = [
            (status(0, 1), Power::BatterySaver),
            (status(1, 1), Power::BatterySaver),
            (status(2 | 8, 1), Power::BatterySaver),
            (status(0, 0), Power::Unrestricted),
            (status(1, 0), Power::Unrestricted),
            (status(128, 1), Power::Unrestricted),
            (status(128, 0), Power::Unrestricted),
            (status(255, 1), Power::Unrestricted),
            (status(0, 2), Power::Unrestricted),
        ];
        for (status, expected) in rows {
            assert_eq!(Power::of(&status), expected, "{status:?}");
        }
    }
}
