use std::path::Path;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use pith_core::{Action, FieldSnapshot, Placement, Rect};
use pith_windows::Reader;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Accessibility::{CUIAutomation8, IUIAutomation, UIA_TextPatternId};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_TAB;
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

use crate::common::{overlay_visible, screenshot, tail, window_rect};
use crate::input::{POLL, focus_field, press, type_text, visible_titles, wait_for_window};
use crate::launch::{Target, launch, marker};
use crate::report::{Case, Row};

const TYPED: &str =
    "Hi Sarah, thanks for sending the revised contract. I reviewed the payment terms and they look";
const FIELD_READY_WAIT: Duration = Duration::from_secs(20);
const TEXT_READ_WAIT: Duration = Duration::from_secs(10);
const SHOW_WAIT: Duration = Duration::from_secs(45);
const READ_WAIT: Duration = Duration::from_secs(5);
const SHOW_SETTLE: Duration = Duration::from_millis(1000);
const ACCEPT_SETTLE: Duration = Duration::from_millis(3000);
const TAIL_CHARS: usize = 40;

pub struct Rig {
    pub actions: Receiver<Action>,
    pub probe: Reader,
    pub probed: Receiver<(Option<FieldSnapshot>, Option<u64>)>,
    pub probe_id: u64,
}

impl Rig {
    fn read(&mut self) -> Option<FieldSnapshot> {
        self.probe_id += 1;
        self.probe.request(self.probe_id);
        let deadline = Instant::now() + READ_WAIT;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match self.probed.recv_timeout(remaining) {
                Ok((snapshot, served)) if served == Some(self.probe_id) => return snapshot,
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        None
    }

    fn read_until(
        &mut self,
        wait: Duration,
        accept: impl Fn(&FieldSnapshot) -> bool,
    ) -> Option<FieldSnapshot> {
        let started = Instant::now();
        let mut last = None;
        while started.elapsed() < wait {
            last = self.read();
            if last.as_ref().is_some_and(&accept) {
                break;
            }
            std::thread::sleep(POLL);
        }
        last
    }

    fn drain(&self) {
        while self.actions.try_recv().is_ok() {}
    }

    fn settle(&self, quiet: Duration) -> Vec<Action> {
        let mut seen = Vec::new();
        while let Ok(action) = self.actions.recv_timeout(quiet) {
            seen.push(action);
        }
        seen
    }

    fn wait_show(&self) -> Option<(Duration, String, Placement)> {
        let started = Instant::now();
        while let Some(remaining) = SHOW_WAIT.checked_sub(started.elapsed()) {
            match self.actions.recv_timeout(remaining) {
                Ok(action) => {
                    if let Some((text, placement)) = shown(&action) {
                        return Some((started.elapsed(), text, placement));
                    }
                }
                Err(_) => return None,
            }
        }
        None
    }
}

fn shown(action: &Action) -> Option<(String, Placement)> {
    match action {
        Action::Show {
            cuts, placement, ..
        } => Some((cuts.first().cloned().unwrap_or_default(), *placement)),
        _ => None,
    }
}

fn latest_show(seen: &[Action]) -> Option<(String, Placement)> {
    seen.iter().rev().find_map(shown)
}

fn inserted(seen: &[Action]) -> String {
    seen.iter()
        .filter_map(|action| match action {
            Action::Insert(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn rect_cell(kind: &str, rect: Rect, window: RECT) -> String {
    let inside = rect.x >= window.left
        && rect.y >= window.top
        && rect.x <= window.right
        && rect.y <= window.bottom;
    let place = if inside { "" } else { ", outside the window" };
    format!(
        "{kind} {},{} {}x{}{place}",
        rect.x, rect.y, rect.width, rect.height
    )
}

fn caret_cell(placement: Option<Placement>, window: RECT) -> String {
    match placement {
        Some(Placement::Inline(rect)) => rect_cell("inline", rect, window),
        Some(Placement::Anchored(rect)) => rect_cell("pill", rect, window),
        Some(Placement::BelowCaret(rect)) => rect_cell("below caret", rect, window),
        None => "none".to_string(),
    }
}

fn whole_screen() -> RECT {
    unsafe {
        RECT {
            left: 0,
            top: 0,
            right: GetSystemMetrics(SM_CXSCREEN),
            bottom: GetSystemMetrics(SM_CYSCREEN),
        }
    }
}

fn focused_element() -> String {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(automation) =
            CoCreateInstance::<_, IUIAutomation>(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
        else {
            return "uia unavailable".to_string();
        };
        match automation.GetFocusedElement() {
            Err(error) => format!("no focused element: {error}"),
            Ok(element) => {
                let class = element
                    .CurrentClassName()
                    .map(|name| name.to_string())
                    .unwrap_or_default();
                let control = element.CurrentControlType().map(|kind| kind.0).unwrap_or(0);
                let text = element.GetCurrentPattern(UIA_TextPatternId).is_ok();
                format!("focused `{class}` control {control} text pattern {text}")
            }
        }
    }
}

fn reads_cell(snapshot: Option<&FieldSnapshot>) -> Result<(), String> {
    match snapshot {
        None => Err("the reader returned nothing".to_string()),
        Some(snapshot) if snapshot.before.trim_end().ends_with(TYPED) => Ok(()),
        Some(snapshot) if snapshot.before.is_empty() => Err("text is empty".to_string()),
        Some(snapshot) => Err(format!("got `{}`", tail(&snapshot.before, TAIL_CHARS))),
    }
}

fn exercise(rig: &mut Rig, hwnd: HWND, out: &Path, shot: &str, row: &mut Row) {
    focus_field(hwnd);
    let window = window_rect(hwnd);
    if rig.read_until(FIELD_READY_WAIT, |_| true).is_none() {
        row.reads = "no".to_string();
        row.fail(format!(
            "no readable field after {} s: {}, {}",
            FIELD_READY_WAIT.as_secs(),
            focused_element(),
            visible_titles()
        ));
        screenshot(whole_screen(), &out.join(format!("{shot}-unreadable.png")));
        return;
    }
    rig.drain();
    type_text(TYPED);
    let typed = rig.read_until(TEXT_READ_WAIT, |snapshot| {
        snapshot.before.trim_end().ends_with(TYPED)
    });
    match reads_cell(typed.as_ref()) {
        Ok(()) => row.reads = "yes".to_string(),
        Err(reason) => {
            row.reads = format!("no, {reason}");
            row.fail(format!("reads: {reason}"));
        }
    }
    row.caret = caret_cell(typed.as_ref().and_then(|s| s.placement), window);
    let before = typed.map(|s| s.before).unwrap_or_default();
    let Some((latency, first, first_placement)) = rig.wait_show() else {
        row.ghost = format!("no, nothing in {} s", SHOW_WAIT.as_secs());
        row.fail("no ghost suggestion");
        return;
    };
    let seen = rig.settle(SHOW_SETTLE);
    let visible = overlay_visible();
    screenshot(whole_screen(), &out.join(format!("{shot}.png")));
    let (text, placement) = latest_show(&seen).unwrap_or((first, first_placement));
    row.ghost = format!(
        "yes `{}`, {} ms, overlay {}{}",
        tail(&text, TAIL_CHARS),
        latency.as_millis(),
        if visible { "visible" } else { "not visible" },
        if matches!(placement, Placement::Anchored(_) | Placement::BelowCaret(_)) {
            ", drawn as a pill"
        } else {
            ""
        }
    );
    if !visible {
        row.fail("overlay not visible");
    }
    rig.drain();
    press(VK_TAB);
    let after_accept = rig.settle(ACCEPT_SETTLE);
    let insert = inserted(&after_accept);
    let after = rig.read_until(TEXT_READ_WAIT, |snapshot| {
        snapshot.before.len() > before.len()
    });
    let landed = after
        .as_ref()
        .is_some_and(|snapshot| snapshot.before.trim_end().ends_with(insert.trim()));
    if insert.is_empty() {
        row.accept = "no, Pith did not insert".to_string();
        row.fail("accept inserted nothing");
    } else if landed {
        row.accept = "yes".to_string();
    } else {
        let got = after.map(|snapshot| tail(&snapshot.before, TAIL_CHARS));
        row.accept = format!("no, expected `{}` got {got:?}", tail(&insert, TAIL_CHARS));
        row.fail("accepted text did not land");
    }
}

pub fn run_case(rig: &mut Rig, target: &Target, case: Case, out: &Path) -> Row {
    let mut row = Row::new(target.app, case);
    let launched = match launch(target, case) {
        Ok(launched) => launched,
        Err(error) => {
            row.fail(format!("launch: {error}"));
            return row;
        }
    };
    let title = marker(target.app, case.field);
    match wait_for_window(&title) {
        Some(hwnd) => {
            let shot = format!("{}-{}-{}", target.app, case.field, case.variant);
            exercise(rig, hwnd, out, &shot, &mut row);
        }
        None => row.fail(format!("no window titled `{title}`: {}", visible_titles())),
    }
    launched.close(&title);
    row
}
