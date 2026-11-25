use std::time::Duration;

use pith_windows::KeyAction;

use crate::harness::actions::{inserted, is_request, is_show};
use crate::harness::rig::Rig;

pub(crate) fn password_field(rig: &mut Rig) {
    let secret = "my very secret passphrase is hunter two";
    let plain = rig.secret;
    rig.fresh(plain, secret);
    let snapshot = rig.read(plain);
    rig.check(
        "control: a single-line control without the password style is read",
        snapshot
            .as_ref()
            .is_some_and(|s| !s.is_password && s.before == secret),
        format!("{:?}", snapshot.map(|s| s.before)),
    );
    let field = rig.locked;
    rig.fresh(field, secret);
    let snapshot = rig.read(field);
    rig.check(
        "password field text is never read",
        snapshot
            .as_ref()
            .is_none_or(|s| s.is_password && s.before.is_empty() && s.after.text().is_empty()),
        format!("{snapshot:?}"),
    );
    rig.drain();
    rig.type_slowly(field.edit, " and more words to type", 20);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "password field never requests or shows",
        !seen.iter().any(is_request) && !seen.iter().any(is_show),
        format!("{seen:?}"),
    );
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(400));
    rig.check(
        "tab in a password field inserts nothing",
        inserted(&seen).is_empty(),
        format!("{seen:?}"),
    );
    rig.use_field(rig.a);
}
