use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::Controls::{TBM_SETPOS, TBM_SETRANGEMAX, TBM_SETRANGEMIN};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetFocus, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    BM_SETCHECK, DestroyWindow, GetDlgCtrlID, GetDlgItem, SWP_NOACTIVATE, SWP_NOZORDER,
    SendMessageW, SetWindowPos, SetWindowTextW,
};
use windows::core::PCWSTR;

use super::layout::{Class, Face, PAGE_ID, Placed};
use super::model::{read, update};
use super::{recorder, slider};
use crate::ui::{Parent, height, width};
use crate::util::wide;

const CHECKED: usize = 1;
const UNCHECKED: usize = 0;

#[derive(Clone, Debug)]
pub struct Spec {
    id: i32,
    class: Class,
    bounds: RECT,
    name: String,
    checked: bool,
    enabled: bool,
    chip: bool,
    slider: Option<(i32, i32, i32)>,
}

impl Spec {
    fn of(placed: &Placed) -> Self {
        let item = &placed.item;
        Self {
            id: placed.id,
            class: item.face.class(),
            bounds: placed.bounds,
            name: item.name.clone(),
            checked: item.checked,
            enabled: item.enabled,
            chip: matches!(item.face, Face::Chip(_)),
            slider: match item.face {
                Face::Slider { value, low, high } => Some((value, low, high)),
                _ => None,
            },
        }
    }
}

pub fn plan() -> Vec<Spec> {
    read(|model| model.layout.controls.iter().map(Spec::of).collect()).unwrap_or_default()
}

fn is_page_control(id: i32) -> bool {
    id >= PAGE_ID
}

fn item(window: HWND, id: i32) -> HWND {
    unsafe { GetDlgItem(Some(window), id) }.unwrap_or_default()
}

fn create(parent: &Parent, spec: &Spec) -> windows::core::Result<HWND> {
    match spec.class {
        Class::Check => parent.check(spec.id, &spec.name, spec.bounds),
        Class::Radio { first } => parent.radio(spec.id, &spec.name, spec.bounds, first),
        Class::Push => {
            let button = parent.button(spec.id, &spec.name, spec.bounds)?;
            if spec.chip {
                recorder::attach(button);
            }
            Ok(button)
        }
        Class::Slider => slider::create(
            parent.hwnd,
            parent.instance,
            (spec.id, &spec.name),
            spec.bounds,
        ),
        Class::Label => parent.label(spec.id, &spec.name, spec.bounds),
    }
}

fn set_text(control: HWND, text: &str) {
    let text = wide(text);
    unsafe {
        let _ = SetWindowTextW(control, PCWSTR(text.as_ptr()));
    }
}

fn sync(control: HWND, spec: &Spec) {
    unsafe {
        let _ = SetWindowPos(
            control,
            None,
            spec.bounds.left,
            spec.bounds.top,
            width(spec.bounds),
            height(spec.bounds),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        let _ = EnableWindow(control, spec.enabled);
    }
    set_text(control, &spec.name);
    match spec.class {
        Class::Check | Class::Radio { .. } => {
            let state = if spec.checked { CHECKED } else { UNCHECKED };
            unsafe {
                SendMessageW(control, BM_SETCHECK, Some(WPARAM(state)), None);
            }
        }
        Class::Slider => {
            if let Some((value, low, high)) = spec.slider {
                set_range(control, (value, low, high));
            }
        }
        Class::Push | Class::Label => {}
    }
}

fn set_range(control: HWND, (value, low, high): (i32, i32, i32)) {
    for (message, number) in [
        (TBM_SETRANGEMIN, low),
        (TBM_SETRANGEMAX, high),
        (TBM_SETPOS, value),
    ] {
        unsafe {
            SendMessageW(
                control,
                message,
                Some(WPARAM(0)),
                Some(LPARAM(number as isize)),
            );
        }
    }
}

fn focused_id(window: HWND) -> Option<i32> {
    let focus = unsafe { GetFocus() };
    let id = unsafe { GetDlgCtrlID(focus) };
    (id != 0 && item(window, id) == focus).then_some(id)
}

pub fn reconcile(window: HWND, instance: HINSTANCE, specs: &[Spec]) {
    let signature: Vec<(i32, Class)> = specs.iter().map(|spec| (spec.id, spec.class)).collect();
    let previous =
        update(|model| std::mem::replace(&mut model.live, signature.clone())).unwrap_or_default();
    let rebuilt = previous != signature;
    let focused = focused_id(window);
    let parent = Parent {
        hwnd: window,
        instance,
    };
    if rebuilt {
        for (id, _) in previous.iter().filter(|(id, _)| is_page_control(*id)) {
            unsafe {
                let _ = DestroyWindow(item(window, *id));
            }
        }
        for spec in specs {
            let kept = !is_page_control(spec.id) && previous.iter().any(|(id, _)| *id == spec.id);
            if !kept {
                let _ = create(&parent, spec);
            }
        }
    }
    for spec in specs {
        sync(item(window, spec.id), spec);
    }
    let target = focused.map(|id| {
        let first_page = specs.iter().find(|spec| is_page_control(spec.id));
        let survives = specs.iter().any(|spec| spec.id == id);
        match (survives, first_page) {
            (false, Some(spec)) => spec.id,
            _ => id,
        }
    });
    if let Some(id) = target {
        unsafe {
            let _ = SetFocus(Some(item(window, id)));
        }
    }
}
