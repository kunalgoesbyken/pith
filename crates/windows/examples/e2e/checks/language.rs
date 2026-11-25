use std::time::Duration;

use pith_windows::KeyAction;

use crate::harness::actions::{inserted, last_show, requests};
use crate::harness::edit::get_text;
use crate::harness::rig::Rig;
use crate::harness::text::tail;

const LANGUAGES: [(&str, &str); 5] = [
    ("japanese", "今日は天気が良いので、公園に"),
    ("hindi", "मुझे कल सुबह दफ्तर जाना है क्योंकि"),
    ("arabic", "أريد أن أشكرك على مساعدتك في"),
    ("emoji", "Let's celebrate 🎉🎉 the launch with"),
    ("mixed", "Meeting notes: आज की बैठक में हमने budget पर"),
];

pub(crate) fn reader_fidelity(rig: &mut Rig) {
    let field = rig.a;
    for (name, text) in LANGUAGES {
        rig.fresh(field, text);
        let snapshot = rig.read(field);
        let before = snapshot
            .as_ref()
            .map(|s| s.before.clone())
            .unwrap_or_default();
        rig.check(
            &format!("reader returns {name} text exactly"),
            before == text,
            format!("{before:?}"),
        );
    }
    rig.fresh(field, "line one\rline two\r");
    let snapshot = rig.read(field);
    rig.check(
        "reader normalizes paragraph marks",
        snapshot
            .as_ref()
            .is_some_and(|s| s.before == "line one\nline two\n"),
        format!("{:?}", snapshot.map(|s| s.before)),
    );
}

pub(crate) fn scripts(rig: &mut Rig) {
    let field = rig.a;
    for (name, text) in LANGUAGES {
        let shown = rig.type_and_wait(field, text, 15);
        let seen = rig.settle(Duration::from_millis(400));
        let suggestion = last_show(&seen).or(shown);
        println!("[INFO] {name}: model suggested {suggestion:?}");
        let Some(suggestion) = suggestion else {
            continue;
        };
        let before = get_text(field.edit);
        rig.key(KeyAction::AcceptAll);
        let seen = rig.settle(Duration::from_millis(500));
        let after = get_text(field.edit);
        rig.check(
            &format!("{name} accept inserts exactly the suggestion"),
            after == format!("{before}{suggestion}") && inserted(&seen) == vec![suggestion.clone()],
            format!("{:?}", tail(&after)),
        );
    }
}

pub(crate) fn prompt_matches_typed_text(rig: &mut Rig) {
    let field = rig.a;
    for (name, text) in LANGUAGES {
        rig.fresh(field, "");
        rig.type_slowly(field.edit, text, 10);
        let seen = rig.settle(Duration::from_millis(1200));
        let Some(request) = requests(&seen).last().map(|r| (*r).clone()) else {
            rig.check(&format!("{name} prompt"), false, "no request".into());
            continue;
        };
        let rebuilt = format!(
            "{}{}",
            request
                .prompt
                .rsplit_once("\n\n")
                .map_or(request.prompt.as_str(), |(_, head)| head),
            request.forced_prefix
        );
        rig.check(
            &format!("{name} prompt plus forced prefix equals the typed text"),
            rebuilt == text,
            format!("{rebuilt:?}"),
        );
    }
}
