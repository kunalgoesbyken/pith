use pith_core::{Chord, Key, KeyAction, Setting};

use super::super::action::Action;
use super::super::layout::{Item, Row, Sheet, Voice};
use super::super::model::Model;
use super::notice;
use crate::ui::{ChipState, Tone};

const RECORDING_HINT: &str = "Press the new keys. Esc cancels, Backspace removes.";

pub(super) fn describe(binding: KeyAction) -> (&'static str, &'static str) {
    match binding {
        KeyAction::AcceptAll => ("Keep the suggestion", "Keeps the whole suggestion."),
        KeyAction::AcceptWord => ("Keep one word", "Keeps only the next word."),
        KeyAction::Trigger => ("Suggest now", "Asks for a suggestion on demand."),
        KeyAction::Dismiss => ("Hide the suggestion", "Always Esc."),
    }
}

fn chip(model: &Model, binding: KeyAction, slot: usize) -> Option<Item> {
    let title = describe(binding).0;
    let chord = model.config.chord(binding, slot);
    let recording = model.recording == Some((binding, slot));
    let first_empty = (0..slot).all(|earlier| model.config.chord(binding, earlier).is_some());
    let (state, label) = match chord {
        _ if recording => (ChipState::Recording, "Press the keys".to_string()),
        Some(chord) => (ChipState::Keys, chord.label()),
        None if first_empty => (ChipState::Empty, "Add a key".to_string()),
        None => return None,
    };
    let name = format!("{title}: {label}");
    Some(Item::chip(
        &name,
        &label,
        state,
        Action::Record(binding, slot),
    ))
}

fn binding_row(model: &Model, binding: KeyAction) -> Row {
    let (title, caption) = describe(binding);
    let recording = matches!(model.recording, Some((active, _)) if active == binding);
    let caption = if recording { RECORDING_HINT } else { caption };
    let chips = (0..binding.slots())
        .filter_map(|slot| chip(model, binding, slot))
        .collect();
    Row::titled(title).caption(caption).buttons(chips)
}

fn escape_row() -> Row {
    let (title, caption) = describe(KeyAction::Dismiss);
    Row::titled(title)
        .caption(caption)
        .keys(vec![Chord::ESCAPE.label()])
}

fn tab_note(model: &Model) -> Option<String> {
    let tab = Chord::plain(Key::Tab);
    model
        .config
        .accept_keys
        .contains(&tab)
        .then(|| "With no suggestion showing, Tab works as a normal Tab.".to_string())
}

pub fn build(model: &Model, sheet: &mut Sheet) {
    sheet.heading("Keys");
    let mut rows: Vec<Row> = KeyAction::REBINDABLE
        .into_iter()
        .map(|binding| binding_row(model, binding))
        .collect();
    rows.push(escape_row());
    sheet.card(rows);
    notice(model, sheet);
    if let Some(note) = tab_note(model) {
        sheet.note(&note, Voice::Quiet);
    }
    sheet.heading("Defaults");
    sheet.card(vec![
        Row::titled("Reset shortcuts")
            .caption("Go back to the keys Pith came with.")
            .control(Item::button(
                "Reset",
                Tone::Standard,
                Action::Apply(Setting::DefaultKeys),
            )),
    ]);
}
