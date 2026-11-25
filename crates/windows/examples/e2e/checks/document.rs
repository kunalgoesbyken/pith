use std::time::{Duration, Instant};

use pith_windows::KeyAction;

use crate::harness::actions::{inserted, is_request, last_show, requests};
use crate::harness::edit::{get_text, select, send_text};
use crate::harness::rig::Rig;
use crate::harness::text::tail;
use crate::harness::window::Field;

const LOOKBACK_CHARS: usize = 19_200;
const MIN_SAMPLES: usize = 5;

const MAX_STEADY_COST: usize = 4;

fn steady_cost(costs: &[usize]) -> usize {
    let mut steady = costs.get(1..).unwrap_or_default().to_vec();
    steady.sort_unstable();
    steady.pop();
    steady.iter().sum::<usize>().div_ceil(steady.len().max(1))
}

fn long_document_text() -> String {
    let paragraph = "The team reviewed the budget and agreed that the schedule needs another week before the launch can go ahead, so please plan accordingly. ";
    (0..40)
        .map(|n| {
            format!(
                "Item {n}: {}{}\r",
                paragraph.repeat(4),
                "Next steps are listed below."
            )
        })
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn keystroke_costs(rig: &Rig, field: Field, text: &str) -> Vec<usize> {
    let mut costs = Vec::new();
    for character in text.chars() {
        let mark = rig.completion_count();
        send_text(field.edit, &character.to_string());
        let deadline = Instant::now() + Duration::from_secs(4);
        while rig.completion_count() == mark && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if let Some(done) = rig.completions_since(mark).first() {
            costs.push(done.prompt_tokens_evaluated);
        }
    }
    costs
}

pub(crate) fn long_document(rig: &mut Rig) {
    let field = rig.a;
    let probe_text = " the quick brown fox jumps over the lazy dog";
    rig.fresh(
        field,
        "Hello team, here is a short note that I am writing today.",
    );
    let short_costs = keystroke_costs(rig, field, probe_text);
    let document = long_document_text();
    let normalized = document.replace('\r', "\n");
    println!(
        "[INFO] long document is {} chars",
        normalized.chars().count()
    );
    rig.fresh(field, &document);
    let snapshot = rig.read(field);
    let before = snapshot
        .as_ref()
        .map(|s| s.before.clone())
        .unwrap_or_default();
    println!(
        "[INFO] long document caret {:?}",
        snapshot.as_ref().map(|s| s.placement)
    );
    rig.check(
        "long document read is a bounded suffix of the document",
        normalized.ends_with(&before)
            && before.chars().count() <= LOOKBACK_CHARS
            && before.chars().count() > 1000,
        format!(
            "{} chars read of {}",
            before.chars().count(),
            normalized.chars().count()
        ),
    );
    let long_costs = keystroke_costs(rig, field, probe_text);
    let (short_mean, long_mean) = (steady_cost(&short_costs), steady_cost(&long_costs));
    rig.check(
        "typing at the end of a long document stays incremental",
        short_costs.len() >= MIN_SAMPLES && long_costs.len() >= MIN_SAMPLES && long_mean <= MAX_STEADY_COST,
        format!("prompt tokens re-evaluated per keystroke after the first: short {short_costs:?} (mean {short_mean}), long {long_costs:?} (mean {long_mean})"),
    );

    let middle = normalized
        .match_indices('\n')
        .nth(20)
        .map(|(i, _)| i)
        .unwrap_or(0);
    select(field.edit, normalized[..middle].encode_utf16().count());
    rig.drain();
    let middle_costs = keystroke_costs(rig, field, " Also remember to");
    let seen = rig.settle(Duration::from_millis(300));
    let middle_mean = steady_cost(&middle_costs);
    rig.check(
        "typing in the middle of a long document stays incremental",
        middle_costs.len() >= MIN_SAMPLES && middle_mean <= MAX_STEADY_COST,
        format!(
            "{middle_costs:?} (mean {middle_mean}), last shown {:?}",
            last_show(&seen)
        ),
    );

    rig.fresh(field, &document);
    let candidates = [
        " Thanks again for the quick turnaround on the",
        " Please let me know if you have any",
        " Quick update on the landing page: the new hero section is live on staging and I would love your",
    ];
    println!(
        "[INFO] caret before the accept flow {:?}",
        rig.read(field).map(|s| s.placement)
    );
    let Some((_, suggestion)) = rig.suggestion_for_long(field, &candidates) else {
        return rig.check("long document accept flow", false, "no suggestion".into());
    };
    let text_before = get_text(field.edit);
    rig.key(KeyAction::AcceptWord);
    let seen = rig.settle(Duration::from_millis(1500));
    let word = inserted(&seen).first().cloned().unwrap_or_default();
    let rest = suggestion[word.len().min(suggestion.len())..].to_string();
    rig.check(
        "long document: accept word keeps the rest visible without a new request",
        rest.trim().is_empty()
            || (last_show(&seen).as_deref() == Some(rest.as_str()) && !seen.iter().any(is_request)),
        format!(
            "suggestion {suggestion:?}, word {word:?}, shown {:?}, requests {}",
            last_show(&seen),
            requests(&seen).len()
        ),
    );
    rig.check(
        "long document: accepted word reached the field",
        get_text(field.edit) == format!("{text_before}{word}"),
        format!("{:?}", tail(&get_text(field.edit))),
    );
    if !rest.trim().is_empty() && last_show(&seen).as_deref() == Some(rest.as_str()) {
        let typed: String = rest.chars().take(2).collect();
        rig.type_slowly(field.edit, &typed, 30);
        let seen = rig.settle(Duration::from_millis(1500));
        rig.check(
            "long document: typing through the suggestion shrinks it without a new request",
            !seen.iter().any(is_request)
                && last_show(&seen).as_deref() == rest.strip_prefix(typed.as_str()),
            format!(
                "rest {rest:?}, typed {typed:?}, shown {:?}, requests {}",
                last_show(&seen),
                requests(&seen).len()
            ),
        );
    }
}

pub(crate) fn read_cost(rig: &mut Rig) {
    let field = rig.a;
    let measure = |rig: &mut Rig, name: &str, text: &str| {
        rig.fresh(field, text);
        let mut times = Vec::new();
        let mut failures = 0;
        for _ in 0..12 {
            let started = Instant::now();
            match rig.read(field) {
                Some(s) if !s.before.is_empty() => times.push(started.elapsed().as_millis()),
                _ => failures += 1,
            }
        }
        times.sort_unstable();
        println!(
            "[INFO] read of {name}: {} ok, {failures} empty or failed, times ms {times:?}",
            times.len()
        );
        (times.len(), failures)
    };
    let (_, short_failures) = measure(
        rig,
        "a short field",
        "Hello team, here is a short note that I am writing today.",
    );
    let (long_ok, long_failures) = measure(rig, "a 23000 char document", &long_document_text());
    rig.check(
        "every read of a long document returns its text",
        short_failures == 0 && long_failures == 0 && long_ok == 12,
        format!("short failures {short_failures}, long failures {long_failures}"),
    );
}
