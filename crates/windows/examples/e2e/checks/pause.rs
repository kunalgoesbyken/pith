use std::time::Duration;

use pith_core::{Action, Config};
use pith_windows::KeyAction;

use crate::common::overlay_visible;
use crate::harness::actions::{
    inserted, is_auto_paused, is_hide, is_request, is_show, last_show, requests,
};
use crate::harness::fixtures::PROSE;
use crate::harness::rig::Rig;

pub(crate) fn auto_pause_on_ignore(rig: &mut Rig) {
    let field = rig.a;
    rig.fresh(field, "");
    let Some(app) = rig.read(field).map(|snapshot| snapshot.app) else {
        return rig.check("auto-pause setup", false, "no snapshot".into());
    };
    let limit = Config::default().auto_pause_after as usize;
    rig.resume(app.clone());
    rig.drain();
    let mut ignored = 0;
    let mut accepted_once = false;
    let mut paused_after = None;
    for attempt in 0..(limit * 3) {
        let text = PROSE[attempt % PROSE.len()];
        let Some(_) = rig.type_and_wait(field, text, 10) else {
            continue;
        };
        std::thread::sleep(Duration::from_millis(200));
        if !accepted_once && ignored == limit / 2 {
            rig.key(KeyAction::AcceptAll);
            let seen = rig.settle(Duration::from_millis(400));
            rig.check(
                "an accept in the middle resets the ignore streak",
                !seen.iter().any(is_auto_paused),
                format!("{seen:?}"),
            );
            accepted_once = true;
            ignored = 0;
            continue;
        }
        rig.key(KeyAction::Dismiss);
        ignored += 1;
        let seen = rig.settle(Duration::from_millis(400));
        if seen.iter().any(is_auto_paused) {
            paused_after = Some(ignored);
            break;
        }
    }
    rig.check(
        "the app pauses after exactly the configured run of ignored suggestions",
        paused_after == Some(limit),
        format!("paused after {paused_after:?}, limit {limit}"),
    );
    rig.fresh(field, "");
    rig.type_slowly(field.edit, PROSE[0], 10);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "a paused app asks the model for nothing and shows nothing",
        !seen.iter().any(is_request) && !seen.iter().any(is_show),
        format!("{seen:?}"),
    );
    rig.resume("not-the-app".into());
    let seen = rig.settle(Duration::from_millis(800));
    rig.check(
        "resuming another app changes nothing",
        !seen.iter().any(is_auto_paused) && !seen.iter().any(is_request),
        format!("{seen:?}"),
    );
    rig.resume(app);
    let seen = rig.settle(Duration::from_millis(2000));
    rig.check(
        "resume reports an empty pause list and asks the model again",
        seen.iter()
            .any(|a| matches!(a, Action::AutoPaused(apps) if apps.is_empty()))
            && seen.iter().any(is_request),
        format!("{seen:?}"),
    );
}

pub(crate) fn pause_and_resume(rig: &mut Rig) {
    let field = rig.a;
    let Some(_) = rig.suggestion_for(field, &PROSE, 1) else {
        return rig.check("pause setup", false, "no suggestion".into());
    };
    rig.set_enabled(false);
    let seen = rig.settle(Duration::from_millis(500));
    rig.check(
        "pause hides the ghost",
        seen.iter().any(is_hide) && !overlay_visible(),
        format!("{seen:?}"),
    );
    rig.type_slowly(field.edit, " and so", 30);
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(1200));
    rig.check(
        "paused: typing requests nothing and tab inserts nothing",
        !seen.iter().any(is_request) && !seen.iter().any(is_show) && inserted(&seen).is_empty(),
        format!("{seen:?}"),
    );
    rig.set_enabled(true);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "resume asks the model again on the unchanged field",
        seen.iter().any(is_request),
        format!(
            "{} requests, shown {:?}",
            requests(&seen).len(),
            last_show(&seen)
        ),
    );
    rig.probe.set_config(&Config {
        enabled: false,
        ..Config::default()
    });
    let paused = rig.read(field);
    rig.probe.set_config(&Config {
        enabled: true,
        ..Config::default()
    });
    let resumed = rig.read(field);
    rig.check(
        "a paused reader reads nothing and a resumed one reads again",
        paused.is_none() && resumed.is_some(),
        format!(
            "paused {paused:?}, resumed {:?}",
            resumed.map(|s| s.before.len())
        ),
    );
}
