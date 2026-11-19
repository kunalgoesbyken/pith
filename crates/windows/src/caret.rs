use std::ffi::c_void;

use pith_core::{Rect, Rgb, TextStyle};
use unicode_segmentation::UnicodeSegmentation;
use windows::Win32::Foundation::{E_FAIL, HWND, POINT};
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

const POINTS_PER_INCH: f64 = 72.0;
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x100_0000_01b3;

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

fn selected_text(element: &IUIAutomationElement) -> Result<String> {
    unsafe {
        let pattern: IUIAutomationTextPattern = element.GetCurrentPatternAs(UIA_TextPatternId)?;
        let range = pattern.GetSelection()?.GetElement(0)?;
        Ok(range.GetText(-1)?.to_string())
    }
}

fn unit_counts(text: &str) -> Vec<i32> {
    let counts = [
        text.graphemes(true).count(),
        text.chars().count(),
        text.encode_utf16().count(),
    ];
    let mut units: Vec<i32> = counts
        .into_iter()
        .filter_map(|count| i32::try_from(count).ok())
        .collect();
    units.dedup();
    units
}

pub fn select_before_caret(element: &IUIAutomationElement, text: &str) -> Result<()> {
    unsafe {
        if has_selection(element) {
            return Err(E_FAIL.into());
        }
        let caret = caret_range(element)?;
        for units in unit_counts(text) {
            let range = caret.Clone()?;
            let moved = range.MoveEndpointByUnit(
                TextPatternRangeEndpoint_Start,
                TextUnit_Character,
                -units,
            )?;
            if moved != -units || range.GetText(-1)? != text {
                continue;
            }
            range.Select()?;
            if selected_text(element).is_ok_and(|selected| selected == text) {
                return Ok(());
            }
            caret.Select()?;
        }
        Err(E_FAIL.into())
    }
}

pub fn collapse_selection(element: &IUIAutomationElement, text: &str) -> Result<()> {
    unsafe {
        if selected_text(element)? != text {
            return Err(E_FAIL.into());
        }
        caret_range(element)?.Select()
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
        take_safe_array::<f64>(array)
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

unsafe fn take_safe_array<T: Copy>(array: *mut SAFEARRAY) -> Vec<T> {
    if array.is_null() {
        return Vec::new();
    }
    unsafe {
        let lower = SafeArrayGetLBound(array, 1).unwrap_or(0);
        let upper = SafeArrayGetUBound(array, 1).unwrap_or(-1);
        let count = (upper - lower + 1).max(0) as usize;
        let mut data: *mut c_void = std::ptr::null_mut();
        let mut values = Vec::new();
        if SafeArrayAccessData(array, &mut data).is_ok() {
            values = std::slice::from_raw_parts(data as *const T, count).to_vec();
            let _ = SafeArrayUnaccessData(array);
        }
        let _ = SafeArrayDestroy(array);
        values
    }
}

pub fn gui_thread_info(window: Option<HWND>) -> Option<GUITHREADINFO> {
    let thread = window.map_or(0, |window| unsafe {
        GetWindowThreadProcessId(window, None)
    });
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetGUIThreadInfo(thread, &mut info) }.ok()?;
    Some(info)
}

fn system_caret(foreground: HWND) -> Option<Rect> {
    let info = gui_thread_info(Some(foreground))?;
    unsafe {
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
    let info = gui_thread_info(Some(foreground))?;
    unsafe {
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

pub fn font(caret: &IUIAutomationTextRange, dpi: u32) -> TextStyle {
    unsafe {
        let Ok(range) = caret.Clone() else {
            return TextStyle::default();
        };
        if range
            .MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -1)
            .unwrap_or(0)
            == 0
        {
            let _ = range.MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Character, 1);
        }
        let family = range
            .GetAttributeValue(UIA_FontNameAttributeId)
            .ok()
            .map(|v| v.to_string())
            .filter(|s| !s.is_empty());
        let px = range
            .GetAttributeValue(UIA_FontSizeAttributeId)
            .ok()
            .and_then(|v| f64::try_from(&v).ok())
            .filter(|points| *points > 0.0)
            .map(|points| (points * f64::from(dpi) / POINTS_PER_INCH) as f32);
        let color = range
            .GetAttributeValue(UIA_ForegroundColorAttributeId)
            .ok()
            .and_then(|v| i32::try_from(&v).ok())
            .map(|color| Rgb::from_bgr(color as u32));
        TextStyle { family, px, color }
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
        take_safe_array::<i32>(array)
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .fold(FNV_OFFSET, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
            })
    }
}
