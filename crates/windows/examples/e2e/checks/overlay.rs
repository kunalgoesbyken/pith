use std::time::Duration;

use pith_windows::KeyAction;
use windows::Win32::Foundation::HWND;

use crate::common::{overlay_visible, overlay_window, window_rect};
use crate::harness::actions::{inserted, visible_text};
use crate::harness::edit::{get_text, text_length};
use crate::harness::fixtures::PROSE;
use crate::harness::rig::Rig;
use crate::harness::screen::{
    caret_screen, monitor_of, monitor_rect, move_caret_off_screen, move_host, pill_body,
};

pub(crate) fn overlay_geometry(rig: &mut Rig) {
    let field = rig.a;
    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 1) else {
        return rig.check("overlay setup", false, "no suggestion".into());
    };
    std::thread::sleep(Duration::from_millis(200));
    let Some(ghost) = overlay_window()
        .map(window_rect)
        .filter(|_| overlay_visible())
    else {
        return rig.check("overlay is visible", false, "no overlay".into());
    };
    let (caret_x, caret_y) = caret_screen(field.edit, text_length(field.edit));
    rig.check(
        "ghost starts right of the caret on the same line",
        ghost.left >= caret_x - 2
            && ghost.left <= caret_x + 12
            && ghost.top <= caret_y + 24
            && ghost.bottom >= caret_y,
        format!(
            "caret ({caret_x},{caret_y}), ghost {:?}, suggestion {suggestion:?}",
            (ghost.left, ghost.top, ghost.right, ghost.bottom)
        ),
    );
    let ghost_monitor = overlay_window().map(monitor_of);
    let field_monitor = Some(monitor_of(HWND(field.edit as _)));
    rig.check(
        "ghost is on the same monitor as the field",
        ghost_monitor == field_monitor && ghost_monitor.is_some_and(|m| !m.is_invalid()),
        format!("{ghost_monitor:?} vs {field_monitor:?}"),
    );
    rig.key(KeyAction::Dismiss);
    rig.settle(Duration::from_millis(300));
    rig.check("ghost hides on escape", !overlay_visible(), String::new());
}

pub(crate) fn overlay_at_screen_edge(rig: &mut Rig) {
    let field = rig.a;
    let monitor = monitor_rect(monitor_of(HWND(field.host as _)));
    let rect = window_rect(HWND(field.host as _));
    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 1) else {
        return rig.check("edge setup", false, "no suggestion".into());
    };
    let (caret_x, _) = caret_screen(field.edit, text_length(field.edit));
    let margin = 24;
    move_host(
        field,
        rect.left + monitor.right - margin - caret_x,
        rect.top,
    );
    std::thread::sleep(Duration::from_millis(500));
    let ghost = overlay_window().map(window_rect);
    let (caret_x, caret_y) = caret_screen(field.edit, text_length(field.edit));
    let overflow = ghost.map(|g| g.right - monitor.right);
    rig.check(
        "ghost near the screen edge stays inside the monitor and clear of the caret",
        ghost.is_some_and(|g| {
            let on_caret =
                (g.left..g.right).contains(&caret_x) && (g.top..g.bottom).contains(&(caret_y + 4));
            g.left >= monitor.left && g.right <= monitor.right && !on_caret
        }),
        format!(
            "caret ({caret_x},{caret_y}), ghost {:?}, monitor {}..{}, suggestion {suggestion:?}",
            ghost.map(|g| (g.left, g.right)),
            monitor.left,
            monitor.right
        ),
    );
    println!("[INFO] ghost overflow past the monitor edge: {overflow:?} px for {suggestion:?}");
    move_host(field, rect.left, rect.top);
}

pub(crate) fn overlay_anchors_without_a_usable_caret(rig: &mut Rig) {
    let field = rig.a;
    let rect = window_rect(HWND(field.host as _));
    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 1) else {
        return rig.check("anchor setup", false, "no suggestion".into());
    };
    let (caret_x, _) = caret_screen(field.edit, text_length(field.edit));
    move_caret_off_screen(field, rect, caret_x);
    std::thread::sleep(Duration::from_millis(500));
    let monitor = monitor_rect(monitor_of(HWND(field.host as _)));
    let edit = window_rect(HWND(field.edit as _));
    let pill = pill_body();
    rig.check(
        "caret off the monitor anchors the suggestion to the field, inside the monitor and clear of its text",
        pill.is_some_and(|p| {
            let clear = p.bottom <= edit.top || p.top >= edit.bottom;
            p.left >= monitor.left
                && p.right <= monitor.right
                && p.top >= monitor.top
                && p.bottom <= monitor.bottom
                && clear
        }),
        format!(
            "pill {:?}, edit {:?}, monitor {:?}, suggestion {suggestion:?}",
            pill.map(|p| (p.left, p.top, p.right, p.bottom)),
            (edit.left, edit.top, edit.right, edit.bottom),
            (monitor.left, monitor.top, monitor.right, monitor.bottom)
        ),
    );
    move_host(field, rect.left, rect.top);
    std::thread::sleep(Duration::from_millis(500));
    let back = overlay_window().map(window_rect);
    let (caret_x, caret_y) = caret_screen(field.edit, text_length(field.edit));
    rig.check(
        "caret back on the monitor returns the ghost inline",
        overlay_visible()
            && back.is_some_and(|g| {
                (g.left - caret_x).abs() <= 12 && g.top <= caret_y + 24 && g.bottom >= caret_y
            }),
        format!(
            "caret ({caret_x},{caret_y}), ghost {:?}",
            back.map(|g| (g.left, g.top, g.right, g.bottom))
        ),
    );
    anchored_accept(rig);
}

fn anchored_accept(rig: &mut Rig) {
    let field = rig.a;
    let rect = window_rect(HWND(field.host as _));
    if rig.suggestion_for(field, &PROSE, 1).is_none() {
        return rig.check("anchored accept setup", false, "no suggestion".into());
    }
    let (caret_x, _) = caret_screen(field.edit, text_length(field.edit));
    move_caret_off_screen(field, rect, caret_x);
    let shown = visible_text(&rig.settle(Duration::from_millis(700)));
    let base = get_text(field.edit);
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(700));
    let landed = inserted(&seen).concat();
    rig.check(
        "accepting from the pill under the field inserts exactly what it showed",
        shown.as_deref() == Some(landed.as_str())
            && get_text(field.edit) == format!("{base}{landed}"),
        format!("shown {shown:?}, inserts {:?}", inserted(&seen)),
    );
    move_host(field, rect.left, rect.top);
}
