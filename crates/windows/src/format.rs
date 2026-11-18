use windows::Win32::Globalization::{
    GetLocaleInfoEx, GetNumberFormatEx, LOCALE_SDECIMAL, LOCALE_SGROUPING, LOCALE_STHOUSAND,
    NUMBERFMTW,
};
use windows::core::{PCWSTR, PWSTR};

use crate::util::wide;

const SECONDS_PER_MINUTE: u64 = 60;
const MINUTES_PER_HOUR: u64 = 60;
const LOCALE_TEXT: usize = 80;
const USER_DEFAULT: PCWSTR = PCWSTR::null();

fn locale_text(kind: u32) -> Option<Vec<u16>> {
    let mut buffer = [0u16; LOCALE_TEXT];
    let length = unsafe { GetLocaleInfoEx(USER_DEFAULT, kind, Some(&mut buffer)) };
    let length = usize::try_from(length).ok().filter(|length| *length > 0)?;
    Some(buffer[..length].to_vec())
}

fn grouping(spec: &str) -> u32 {
    let mut sizes: Vec<&str> = spec.split(';').collect();
    if sizes.last() == Some(&"0") {
        sizes.pop();
    } else {
        sizes.push("0");
    }
    sizes.concat().parse().unwrap_or(0)
}

fn localized(digits: &str) -> Option<String> {
    let mut decimal = locale_text(LOCALE_SDECIMAL)?;
    let mut thousand = locale_text(LOCALE_STHOUSAND)?;
    let spec = locale_text(LOCALE_SGROUPING)?;
    let spec = String::from_utf16_lossy(&spec[..spec.len() - 1]);
    let format = NUMBERFMTW {
        NumDigits: 0,
        LeadingZero: 0,
        Grouping: grouping(&spec),
        lpDecimalSep: PWSTR(decimal.as_mut_ptr()),
        lpThousandSep: PWSTR(thousand.as_mut_ptr()),
        NegativeOrder: 1,
    };
    let value = wide(digits);
    let mut buffer = [0u16; LOCALE_TEXT];
    let length = unsafe {
        GetNumberFormatEx(
            USER_DEFAULT,
            0,
            PCWSTR(value.as_ptr()),
            Some(&format),
            Some(&mut buffer),
        )
    };
    let length = usize::try_from(length).ok().filter(|length| *length > 1)?;
    Some(String::from_utf16_lossy(&buffer[..length - 1]))
}

pub fn number(value: u64) -> String {
    let digits = value.to_string();
    localized(&digits).unwrap_or(digits)
}

pub fn duration(seconds: u64, number: impl Fn(u64) -> String) -> String {
    let minutes = seconds / SECONDS_PER_MINUTE;
    match (minutes / MINUTES_PER_HOUR, minutes % MINUTES_PER_HOUR) {
        (0, 0) if seconds == 0 => format!("{} min", number(0)),
        (0, 0) => format!("< {} min", number(1)),
        (0, minutes) => format!("{} min", number(minutes)),
        (hours, 0) => format!("{} h", number(hours)),
        (hours, minutes) => format!("{} h {} min", number(hours), number(minutes)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping_specs_follow_the_numberfmt_encoding() {
        for (spec, packed) in [("3;0", 3), ("3;2;0", 32), ("3", 30), ("0", 0), ("", 0)] {
            assert_eq!(grouping(spec), packed, "{spec:?}");
        }
    }

    #[test]
    fn numbers_keep_every_digit_and_never_gain_a_fraction() {
        for value in [0, 7, 999, 1_000, 12_345, 1_234_567, u64::MAX] {
            let text = number(value);
            let digits: String = text.chars().filter(char::is_ascii_digit).collect();
            assert_eq!(digits, value.to_string(), "{text}");
        }
    }

    #[test]
    fn durations_round_down_to_whole_minutes() {
        let plain = |value: u64| value.to_string();
        for (seconds, label) in [
            (0, "0 min"),
            (1, "< 1 min"),
            (59, "< 1 min"),
            (60, "1 min"),
            (3_599, "59 min"),
            (3_600, "1 h"),
            (3_660, "1 h 1 min"),
            (90_000, "25 h"),
        ] {
            assert_eq!(duration(seconds, plain), label, "{seconds}");
        }
    }
}
