use std::path::PathBuf;

use pith_core::Reach;
use pith_windows::{Onboarding, OnboardingEvent, OnboardingStep, Outcome};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::common::window_title;
use crate::harness::edit::get_text;
use crate::harness::rig::Report;
use crate::harness::window::ES_MULTILINE;

fn shown(hwnd: HWND) -> bool {
    unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_VISIBLE.0 != 0 }
}

fn onboarding_flow(marker: PathBuf) -> usize {
    let mut report = Report {
        passed: 0,
        failed: 0,
    };
    let mut window =
        Onboarding::create(marker.clone(), false, &Reach::ThisPc).expect("onboarding window");
    let controls = window.controls();
    report.check(
        "onboarding starts quiet",
        window_title(controls.status) == "Starting Pith"
            && !shown(controls.progress)
            && !shown(controls.practice)
            && !unsafe { IsWindowEnabled(controls.done).as_bool() },
        window_title(controls.status),
    );
    window.event(OnboardingEvent::Progress(150, 400));
    let position = window.step().permille();
    report.check(
        "onboarding shows download progress",
        shown(controls.progress)
            && position == Some(375)
            && window_title(controls.status).contains(" / ")
            && window_title(controls.status).ends_with("MB"),
        format!("{} at {position:?}", window_title(controls.status)),
    );
    window.event(OnboardingEvent::Progress(400, 400));
    report.check(
        "onboarding reports loading",
        window.step() == OnboardingStep::Loading
            && window_title(controls.status) == "Loading the model"
            && !shown(controls.progress),
        window_title(controls.status),
    );
    window.event(OnboardingEvent::Ready);
    let seeded = get_text(controls.practice.0 as isize);
    report.check(
        "onboarding opens the practice box with a sentence start",
        window_title(controls.status) == "Ready"
            && shown(controls.practice)
            && unsafe { IsWindowEnabled(controls.practice).as_bool() }
            && !seeded.is_empty()
            && !seeded.ends_with(char::is_whitespace),
        seeded.clone(),
    );
    let keys = window_title(controls.keys);
    report.check(
        "onboarding lists the keys",
        ["Tab", "Shift+Tab", "Esc"]
            .iter()
            .all(|key| keys.contains(key)),
        keys,
    );
    let style = unsafe { GetWindowLongPtrW(controls.practice, GWL_STYLE) } as u32;
    report.check(
        "practice box is multi-line",
        style & ES_MULTILINE != 0,
        format!("{style:#x}"),
    );
    report.check(
        "polling before Done stays pending without a marker",
        window.poll() == Outcome::Pending && !marker.exists(),
        format!("{:?}", window.step()),
    );
    unsafe {
        SendMessageW(controls.done, BM_CLICK, None, None);
    }
    let outcome = window.poll();
    report.check(
        "done writes the marker and finishes",
        outcome == Outcome::Finished && marker.exists() && !shown(controls.window),
        format!("{outcome:?}"),
    );
    drop(window);
    report.failed
}

fn onboarding_failure() -> usize {
    let mut report = Report {
        passed: 0,
        failed: 0,
    };
    let marker = std::env::temp_dir().join("pith-e2e-onboarding-failed");
    let mut window =
        Onboarding::create(marker.clone(), false, &Reach::ThisPc).expect("onboarding window");
    let controls = window.controls();
    window.event(OnboardingEvent::Failed("offline & retry".into()));
    report.check(
        "onboarding shows a startup failure",
        window_title(controls.status).contains("offline & retry")
            && !shown(controls.practice)
            && unsafe { IsWindowEnabled(controls.done).as_bool() },
        window_title(controls.status),
    );
    unsafe {
        SendMessageW(controls.done, BM_CLICK, None, None);
    }
    report.check(
        "closing after failure leaves no marker",
        window.poll() == Outcome::Closed && !marker.exists(),
        format!("{:?}", window.step()),
    );
    report.failed
}

pub(crate) fn onboarding_checks() -> usize {
    let marker = std::env::temp_dir().join("pith-e2e-onboarded");
    let _ = std::fs::remove_file(&marker);
    let flow = marker.clone();
    let failed = std::thread::spawn(move || onboarding_flow(flow) + onboarding_failure())
        .join()
        .unwrap_or(1);
    let _ = std::fs::remove_file(&marker);
    failed
}
