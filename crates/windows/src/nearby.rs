use std::time::Instant;

use pith_core::{CueKind, RawCue, WALK, log};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::*;
use windows::core::Result;

struct Node {
    element: IUIAutomationElement,
    kind: UIA_CONTROLTYPE_ID,
    name: String,
    value: String,
    rect: RECT,
}

const TEXT_KINDS: [UIA_CONTROLTYPE_ID; 3] = [
    UIA_TextControlTypeId,
    UIA_HeaderControlTypeId,
    UIA_HeaderItemControlTypeId,
];
const VALUE_KINDS: [UIA_CONTROLTYPE_ID; 2] = [UIA_EditControlTypeId, UIA_ComboBoxControlTypeId];
const THREAD_KINDS: [UIA_CONTROLTYPE_ID; 4] = [
    UIA_ListControlTypeId,
    UIA_DocumentControlTypeId,
    UIA_DataGridControlTypeId,
    UIA_TableControlTypeId,
];

impl Node {
    fn cue(&self) -> Option<RawCue> {
        if TEXT_KINDS.contains(&self.kind) {
            return Some(RawCue::new(CueKind::Text, None, &self.name));
        }
        (VALUE_KINDS.contains(&self.kind) && !self.value.is_empty())
            .then(|| RawCue::new(CueKind::FormValue, Some(&self.name), &self.value))
    }

    fn is_thread_above(&self, field: &RECT) -> bool {
        let overlap = self.rect.right.min(field.right) - self.rect.left.max(field.left);
        THREAD_KINDS.contains(&self.kind)
            && overlap > 0
            && overlap >= (field.right - field.left) / 2
    }
}

struct Walk<'a> {
    automation: &'a IUIAutomation,
    walker: IUIAutomationTreeWalker,
    request: IUIAutomationCacheRequest,
    anything: IUIAutomationCondition,
    texts: IUIAutomationCondition,
    root: HWND,
    started: Instant,
    calls: u32,
}

pub fn read(automation: &IUIAutomation, field: &IUIAutomationElement, root: HWND) -> Vec<RawCue> {
    let started = Instant::now();
    let Ok(mut walk) = Walk::new(automation, root) else {
        return Vec::new();
    };
    let cues = walk.run(field);
    log(&format!(
        "nearby: {} cues, {} calls, {} ms",
        cues.len(),
        walk.calls,
        started.elapsed().as_millis()
    ));
    cues
}

impl<'a> Walk<'a> {
    fn new(automation: &'a IUIAutomation, root: HWND) -> Result<Self> {
        unsafe {
            let request = automation.CreateCacheRequest()?;
            for property in [
                UIA_NamePropertyId,
                UIA_ControlTypePropertyId,
                UIA_IsPasswordPropertyId,
                UIA_IsOffscreenPropertyId,
                UIA_HelpTextPropertyId,
                UIA_RuntimeIdPropertyId,
                UIA_BoundingRectanglePropertyId,
                UIA_NativeWindowHandlePropertyId,
                UIA_ValueValuePropertyId,
            ] {
                request.AddProperty(property)?;
            }
            request.AddPattern(UIA_ValuePatternId)?;
            let text_type = VARIANT::from(UIA_TextControlTypeId.0);
            Ok(Self {
                automation,
                walker: automation.ControlViewWalker()?,
                request,
                anything: automation.CreateTrueCondition()?,
                texts: automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &text_type)?,
                root,
                started: Instant::now(),
                calls: 0,
            })
        }
    }

    fn step(&mut self) -> bool {
        if self.started.elapsed() >= WALK.deadline {
            return false;
        }
        self.calls += 1;
        true
    }

    fn run(&mut self, field: &IUIAutomationElement) -> Vec<RawCue> {
        let mut cues = Vec::new();
        let Some(mut current) = self.refresh(field) else {
            return cues;
        };
        let Ok(field_rect) = (unsafe { current.CachedBoundingRectangle() }) else {
            return cues;
        };
        cues.extend(self.labels(field, &current));
        let mut thread_found = false;
        for _ in 0..WALK.levels {
            let Some(parent) = self.parent(&current) else {
                break;
            };
            for sibling in self.preceding(&parent, &current) {
                if !thread_found && sibling.is_thread_above(&field_rect) {
                    thread_found = true;
                    cues.extend(self.latest_message(&sibling));
                } else {
                    cues.extend(sibling.cue());
                }
            }
            if self.is_root(&parent) {
                break;
            }
            current = parent;
        }
        cues
    }

    fn is_root(&self, element: &IUIAutomationElement) -> bool {
        unsafe { element.CachedNativeWindowHandle() }.is_ok_and(|handle| handle == self.root)
    }

    fn refresh(&mut self, element: &IUIAutomationElement) -> Option<IUIAutomationElement> {
        if !self.step() {
            return None;
        }
        unsafe { element.BuildUpdatedCache(&self.request) }.ok()
    }

    fn parent(&mut self, element: &IUIAutomationElement) -> Option<IUIAutomationElement> {
        if !self.step() {
            return None;
        }
        unsafe {
            self.walker
                .GetParentElementBuildCache(element, &self.request)
        }
        .ok()
    }

    fn node(&self, element: &IUIAutomationElement) -> Option<Node> {
        unsafe {
            if element.CachedIsPassword().ok()?.as_bool()
                || element.CachedIsOffscreen().ok()?.as_bool()
            {
                return None;
            }
            let value = element
                .GetCachedPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .and_then(|pattern| pattern.CachedValue())
                .map(|value| value.to_string())
                .unwrap_or_default();
            Some(Node {
                element: element.clone(),
                kind: element.CachedControlType().ok()?,
                name: element.CachedName().ok()?.to_string(),
                value,
                rect: element.CachedBoundingRectangle().ok()?,
            })
        }
    }

    fn labels(
        &mut self,
        field: &IUIAutomationElement,
        cached: &IUIAutomationElement,
    ) -> Vec<RawCue> {
        let labeled_by = if self.step() {
            unsafe { field.CurrentLabeledBy() }
                .and_then(|label| unsafe { label.CurrentName() })
                .map(|name| name.to_string())
                .ok()
        } else {
            None
        };
        let described = unsafe {
            [
                cached.CachedName().map(|name| name.to_string()),
                cached.CachedHelpText().map(|help| help.to_string()),
            ]
        };
        labeled_by
            .into_iter()
            .chain(described.into_iter().flatten())
            .map(|text| RawCue::new(CueKind::FieldLabel, None, &text))
            .collect()
    }

    fn preceding(
        &mut self,
        parent: &IUIAutomationElement,
        current: &IUIAutomationElement,
    ) -> Vec<Node> {
        if !self.step() {
            return Vec::new();
        }
        unsafe {
            let Ok(children) =
                parent.FindAllBuildCache(TreeScope_Children, &self.anything, &self.request)
            else {
                return Vec::new();
            };
            let count = children.Length().unwrap_or(0);
            let all: Vec<IUIAutomationElement> = (0..count)
                .filter_map(|index| children.GetElement(index).ok())
                .collect();
            let Some(position) = all.iter().position(|child| {
                self.automation
                    .CompareElements(current, child)
                    .is_ok_and(|same| same.as_bool())
            }) else {
                return Vec::new();
            };
            all[..position]
                .iter()
                .rev()
                .take(WALK.siblings)
                .filter_map(|child| self.node(child))
                .collect()
        }
    }

    fn latest_message(&mut self, thread: &Node) -> Option<RawCue> {
        let mut latest = thread.element.clone();
        for _ in 0..WALK.descent {
            if !self.step() {
                break;
            }
            let Ok(child) = (unsafe {
                self.walker
                    .GetLastChildElementBuildCache(&latest, &self.request)
            }) else {
                break;
            };
            latest = child;
        }
        if !self.step() {
            return None;
        }
        let lines = unsafe {
            let found = latest
                .FindAllBuildCache(TreeScope_Descendants, &self.texts, &self.request)
                .ok()?;
            (0..found.Length().unwrap_or(0))
                .filter_map(|index| found.GetElement(index).ok())
                .filter_map(|text| text.CachedName().ok())
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
        };
        let own_name = unsafe { latest.CachedName() }
            .map(|name| name.to_string())
            .unwrap_or_default();
        let text = match lines.len() {
            0 => own_name,
            count => lines[count.saturating_sub(WALK.siblings)..].join(" "),
        };
        Some(RawCue::new(CueKind::Message, None, &text))
    }
}
