use std::time::{Duration, Instant};

use pith_windows::KeyAction;

use crate::harness::actions::inserted;
use crate::harness::edit::get_text;
use crate::harness::fixtures::PROSE;
use crate::harness::memory::print_memory;
use crate::harness::rig::Rig;

pub(crate) fn idle_unload(rig: &mut Rig, unload_after: Duration) {
    let field = rig.a;
    rig.fresh(field, "");
    let loaded = rig.suggestion_for(field, &PROSE, 1).is_some();
    rig.fresh(field, "");
    while rig.statuses.try_recv().is_ok() {}
    rig.check(
        "model is loaded before the idle period",
        loaded,
        String::new(),
    );
    print_memory("loaded, after typing");
    let unloaded = loop {
        match rig
            .statuses
            .recv_timeout(unload_after + Duration::from_secs(8))
        {
            Ok(status) if status.contains("unloaded") => break true,
            Ok(_) => {}
            Err(_) => break false,
        }
    };
    rig.check(
        "model unloads after the idle period",
        unloaded,
        String::new(),
    );
    std::thread::sleep(Duration::from_millis(800));
    print_memory("after idle unload");
    rig.type_slowly(field.edit, PROSE[2], 10);
    let started = Instant::now();
    let shown = rig.wait_show(Duration::from_secs(15));
    rig.check(
        "suggestion returns after the model was unloaded",
        shown.is_some(),
        format!("{shown:?} after {:?}", started.elapsed()),
    );
    print_memory("after reload");
    let text = get_text(field.edit);
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(500));
    rig.check(
        "accept works right after a reload",
        shown
            .as_deref()
            .is_some_and(|s| get_text(field.edit) == format!("{text}{s}")),
        format!("{:?}", inserted(&seen)),
    );
}
