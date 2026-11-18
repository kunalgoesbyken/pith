use std::ffi::c_void;

use pith_core::Rect;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::MapWindowPoints;
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Ole::{
    SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetLBound, SafeArrayGetUBound,
    SafeArrayUnaccessData,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::OBJID_CARET;
use windows::Win32::UI::WindowsAndMessaging::{
    GUITHREADINFO, GetGUIThreadInfo, GetWindowThreadProcessId,
};
use windows::core::{BOOL, Interface, Result};

pub fn caret_range(element: &IUIAutomationElement) -> Result<IUIAutomationTextRange> {
    unsafe {
        if let Ok(pattern) =
            element.GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id)
        {
            let mut active = BOOL(0);
            if let Ok(range) = pattern.GetCaretRange(&mut active) {
                return Ok(range);
            }
        }
        let pattern: IUIAutomationTextPattern = element.GetCurrentPatternAs(UIA_TextPatternId)?;
        let range = pattern.GetSelection()?.GetElement(0)?;
        range.MoveEndpointByRange(
            TextPatternRangeEndpoint_Start,
            &range,
            TextPatternRangeEndpoint_End,
        )?;
        Ok(range)
    }
}

pub fn has_selection(element: &IUIAutomationElement) -> bool {
    unsafe {
        let Ok(pattern) =
            element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        else {
            return false;
        };
        let Ok(selection) = pattern.GetSelection() else {
            return false;
        };
        let Ok(range) = selection.GetElement(0) else {
            return false;
        };
        range
            .CompareEndpoints(
                TextPatternRangeEndpoint_Start,
                &range,
                TextPatternRangeEndpoint_End,
            )
            .map(|d| d != 0)
            .unwrap_or(false)
    }
}

pub fn text_around(
    caret: &IUIAutomationTextRange,
    before: i32,
    after: i32,
) -> Result<(String, String)> {
    unsafe {
        let left = caret.Clone()?;
        left.MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -before)?;
        let right = caret.Clone()?;
        right.MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Character, after)?;
        Ok((
            left.GetText(-1)?.to_string(),
            right.GetText(-1)?.to_string(),
        ))
    }
}

fn rects(range: &IUIAutomationTextRange) -> Vec<Rect> {
    unsafe {
        let Ok(array) = range.GetBoundingRectangles() else {
            return Vec::new();
        };
        let array: *mut SAFEARRAY = array;
        if array.is_null() {
            return Vec::new();
        }
        let lower = SafeArrayGetLBound(array, 1).unwrap_or(0);
        let upper = SafeArrayGetUBound(array, 1).unwrap_or(-1);
        let count = (upper - lower + 1).max(0) as usize;
        let mut data: *mut c_void = std::ptr::null_mut();
        let mut values = Vec::new();
        if SafeArrayAccessData(array, &mut data).is_ok() {
            values = std::slice::from_raw_parts(data as *const f64, count).to_vec();
            let _ = SafeArrayUnaccessData(array);
        }
        let _ = SafeArrayDestroy(array);
        values
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| Rect {
                x: c[0].round() as i32,
                y: c[1].round() as i32,
                width: c[2].round() as i32,
                height: c[3].round() as i32,
            })
            .collect()
    }
}

fn uia_caret(caret: &IUIAutomationTextRange) -> Option<Rect> {
    if let Some(rect) = rects(caret).into_iter().next().filter(|r| r.height > 0) {
        return Some(rect);
    }
    unsafe {
        let previous = caret.Clone().ok()?;
        if previous
            .MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -1)
            .unwrap_or(0)
            == -1
        {
            let text = previous
                .GetText(-1)
                .map(|s| s.to_string())
                .unwrap_or_default();
            if !text.ends_with(['\r', '\n'])
                && let Some(last) = rects(&previous).into_iter().last()
            {
                return Some(Rect {
                    x: last.x + last.width,
                    y: last.y,
                    width: 1,
                    height: last.height,
                });
            }
        }
        let next = caret.Clone().ok()?;
        if next
            .MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Character, 1)
            .unwrap_or(0)
            == 1
            && let Some(first) = rects(&next).into_iter().next()
        {
            return Some(Rect {
                x: first.x,
                y: first.y,
                width: 1,
                height: first.height,
            });
        }
    }
    None
}

fn system_caret(foreground: HWND) -> Option<Rect> {
    unsafe {
        let thread = GetWindowThreadProcessId(foreground, None);
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(thread, &mut info).ok()?;
        if info.hwndCaret.is_invalid() {
            return None;
        }
        let r = info.rcCaret;
        let mut points = [
            POINT {
                x: r.left,
                y: r.top,
            },
            POINT {
                x: r.right,
                y: r.bottom,
            },
        ];
        MapWindowPoints(Some(info.hwndCaret), None, &mut points);
        let height = points[1].y - points[0].y;
        (height > 0).then(|| Rect {
            x: points[0].x,
            y: points[0].y,
            width: (points[1].x - points[0].x).max(1),
            height,
        })
    }
}

fn accessible_caret(foreground: HWND) -> Option<Rect> {
    unsafe {
        let thread = GetWindowThreadProcessId(foreground, None);
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(thread, &mut info).ok()?;
        let window = if info.hwndFocus.is_invalid() {
            foreground
        } else {
            info.hwndFocus
        };
        let mut raw: *mut c_void = std::ptr::null_mut();
        AccessibleObjectFromWindow(window, OBJID_CARET.0 as u32, &IAccessible::IID, &mut raw)
            .ok()?;
        if raw.is_null() {
            return None;
        }
        let accessible = IAccessible::from_raw(raw);
        let (mut x, mut y, mut width, mut height) = (0, 0, 0, 0);
        accessible
            .accLocation(
                &mut x,
                &mut y,
                &mut width,
                &mut height,
                &VARIANT::from(0i32),
            )
            .ok()?;
        (height > 0).then_some(Rect {
            x,
            y,
            width: width.max(1),
            height,
        })
    }
}

pub fn caret_rect(caret: &IUIAutomationTextRange, foreground: Option<HWND>) -> Option<Rect> {
    let uia = uia_caret(caret);
    let system = foreground.and_then(system_caret);
    match (uia, system) {
        (Some(u), Some(s)) if (u.x - s.x).abs() <= 6 && s.height < u.height => Some(s),
        (Some(u), _) => Some(u),
        (None, Some(s)) => Some(s),
        (None, None) => foreground.and_then(accessible_caret),
    }
}

pub fn font(caret: &IUIAutomationTextRange) -> (Option<String>, Option<f64>, Option<u32>) {
    unsafe {
        let Ok(range) = caret.Clone() else {
            return (None, None, None);
        };
        if range
            .MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -1)
            .unwrap_or(0)
            == 0
        {
            let _ = range.MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Character, 1);
        }
        let name = range
            .GetAttributeValue(UIA_FontNameAttributeId)
            .ok()
            .map(|v| v.to_string())
            .filter(|s| !s.is_empty());
        let size = range
            .GetAttributeValue(UIA_FontSizeAttributeId)
            .ok()
            .and_then(|v| f64::try_from(&v).ok())
            .filter(|s| *s > 0.0);
        let color = range
            .GetAttributeValue(UIA_ForegroundColorAttributeId)
            .ok()
            .and_then(|v| i32::try_from(&v).ok())
            .map(|c| c as u32 & 0x00FF_FFFF);
        (name, size, color)
    }
}

pub fn composing(element: &IUIAutomationElement) -> bool {
    unsafe {
        let Ok(pattern) =
            element.GetCurrentPatternAs::<IUIAutomationTextEditPattern>(UIA_TextEditPatternId)
        else {
            return false;
        };
        let Ok(range) = pattern.GetActiveComposition() else {
            return false;
        };
        range.GetText(-1).map(|t| !t.is_empty()).unwrap_or(false)
    }
}

pub fn runtime_id(element: &IUIAutomationElement) -> u64 {
    unsafe {
        let Ok(array) = element.GetRuntimeId() else {
            return 0;
        };
        let array: *mut SAFEARRAY = array;
        if array.is_null() {
            return 0;
        }
        let lower = SafeArrayGetLBound(array, 1).unwrap_or(0);
        let upper = SafeArrayGetUBound(array, 1).unwrap_or(-1);
        let count = (upper - lower + 1).max(0) as usize;
        let mut data: *mut c_void = std::ptr::null_mut();
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        if SafeArrayAccessData(array, &mut data).is_ok() {
            for value in std::slice::from_raw_parts(data as *const i32, count) {
                for byte in value.to_le_bytes() {
                    hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
                }
            }
            let _ = SafeArrayUnaccessData(array);
        }
        let _ = SafeArrayDestroy(array);
        hash
    }
}
