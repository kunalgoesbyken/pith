use crate::{Chord, Config, KeyAction};

impl KeyAction {
    pub const ALL: [Self; 4] = [
        Self::AcceptAll,
        Self::AcceptWord,
        Self::Trigger,
        Self::Dismiss,
    ];
    pub const REBINDABLE: [Self; 3] = [Self::AcceptAll, Self::AcceptWord, Self::Trigger];

    pub const fn slots(self) -> usize {
        match self {
            Self::AcceptAll | Self::AcceptWord => 2,
            Self::Trigger => 1,
            Self::Dismiss => 1,
        }
    }
}

pub(crate) fn ensure_bindable(action: KeyAction, chord: Option<Chord>) -> Result<(), String> {
    if action == KeyAction::Dismiss {
        return Err("Esc always hides the suggestion.".into());
    }
    let Some(chord) = chord else {
        return Ok(());
    };
    if chord.win {
        return Err("Windows keys are reserved by Windows. Pick another key.".into());
    }
    chord.ensure_safe(&chord.to_string())
}

fn replace_at(chords: &mut Vec<Chord>, slot: usize, chord: Option<Chord>) {
    match chord {
        Some(chord) if slot < chords.len() => chords[slot] = chord,
        Some(chord) => chords.push(chord),
        None if slot < chords.len() => {
            chords.remove(slot);
        }
        None => {}
    }
}

impl Config {
    pub fn chord(&self, action: KeyAction, slot: usize) -> Option<Chord> {
        match action {
            KeyAction::AcceptAll => self.accept_keys.get(slot).copied(),
            KeyAction::AcceptWord => self.accept_word_keys.get(slot).copied(),
            KeyAction::Dismiss => (slot == 0).then_some(Chord::ESCAPE),
            KeyAction::Trigger => self
                .force_trigger
                .then_some(self.force_trigger_key)
                .filter(|_| slot == 0),
        }
    }

    pub(crate) fn set_chord(&mut self, action: KeyAction, slot: usize, chord: Option<Chord>) {
        match (action, chord) {
            (KeyAction::AcceptAll, chord) => replace_at(&mut self.accept_keys, slot, chord),
            (KeyAction::AcceptWord, chord) => replace_at(&mut self.accept_word_keys, slot, chord),
            (KeyAction::Trigger, Some(chord)) => {
                self.force_trigger_key = chord;
                self.force_trigger = true;
            }
            (KeyAction::Trigger, None) => self.force_trigger = false,
            (KeyAction::Dismiss, _) => {}
        }
    }

    pub fn default_keys(&self) -> Self {
        let defaults = Self::default();
        Self {
            accept_keys: defaults.accept_keys,
            accept_word_keys: defaults.accept_word_keys,
            force_trigger: defaults.force_trigger,
            force_trigger_key: defaults.force_trigger_key,
            ..self.clone()
        }
    }
}
