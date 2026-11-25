use std::time::{Duration, Instant};

use pith_core::{Config, Primer, WALK, build};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use crate::harness::rig::Rig;

pub(crate) fn nearby_context(rig: &mut Rig) {
    let form = rig.form;
    let on = Config {
        nearby_context: true,
        ..Config::default()
    };
    let off = Config {
        nearby_context: false,
        ..Config::default()
    };
    rig.probe.set_config(&off);
    let started = Instant::now();
    let plain = rig.read(form);
    let plain_cost = started.elapsed();
    rig.check(
        "nearby text is not read while the setting is off",
        plain.as_ref().is_some_and(|s| s.nearby.is_empty()),
        format!("{:?}", plain.map(|s| s.nearby)),
    );
    rig.probe.set_config(&on);
    rig.read(rig.a);
    let started = Instant::now();
    let cold = rig.read(form);
    let walk_cost = started.elapsed().saturating_sub(plain_cost);
    let prompt = cold
        .as_ref()
        .map(|s| build(s, &on, &Primer::default()).prompt);
    rig.check(
        "the prompt carries the subject beside the box",
        prompt
            .as_ref()
            .is_some_and(|p| p.contains("Quarterly numbers")),
        format!("{prompt:?}"),
    );
    rig.check(
        "the password box never reaches the prompt or the cues",
        cold.is_some()
            && !format!("{:?}", cold.as_ref().map(|s| &s.nearby)).contains("hunter2")
            && !prompt.as_deref().unwrap_or_default().contains("hunter2"),
        format!("{prompt:?}"),
    );
    rig.check(
        "the walk stays inside its time budget",
        walk_cost <= WALK.deadline + Duration::from_millis(40),
        format!("{walk_cost:?}"),
    );
    let subject = unsafe {
        FindWindowExW(
            Some(HWND(form.host as _)),
            None,
            w!("EDIT"),
            w!("Quarterly numbers"),
        )
        .expect("subject box")
    };
    unsafe { SetWindowTextW(subject, w!("Annual review")).expect("retitle subject") };
    let cached = rig.read(form);
    rig.check(
        "the same field reuses its cues instead of walking again",
        cached.as_ref().map(|s| &s.nearby) == cold.as_ref().map(|s| &s.nearby),
        format!("{:?}", cached.map(|s| s.nearby)),
    );
    rig.read(rig.a);
    let fresh = rig
        .read(form)
        .map(|s| build(&s, &on, &Primer::default()).prompt);
    rig.check(
        "switching fields walks the new field again",
        fresh.as_ref().is_some_and(|p| p.contains("Annual review")),
        format!("{fresh:?}"),
    );
    let blocked = Config {
        blocked_apps: cold.iter().map(|s| s.app.clone()).collect(),
        ..on
    };
    rig.probe.set_config(&blocked);
    let guarded = rig.read(form);
    rig.check(
        "a blocked app yields no cues",
        guarded.as_ref().is_none_or(|s| s.nearby.is_empty()),
        format!("{:?}", guarded.map(|s| s.nearby)),
    );
    rig.probe.set_config(&Config::default());
    unsafe { SetWindowTextW(subject, w!("Quarterly numbers")).expect("restore subject") };
    rig.use_field(rig.a);
}
