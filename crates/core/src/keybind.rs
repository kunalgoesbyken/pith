use crate::{Chord, Config, KeyAction};

impl KeyAction {
    pub const REBINDABLE: [Self; 3] = [Self::AcceptAll, Self::AcceptWord, Self::Trigger];

    pub const fn slots(self) -> usize {
        match self {
            Self::AcceptAll | Self::AcceptWord => 2,
            Self::Trigger => 1,
            Self::Dismiss => 0,
        }
    }
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
            KeyAction::Dismiss => Some(Chord::ESCAPE).filter(|_| slot == 0),
            KeyAction::Trigger => self
                .force_trigger
                .then_some(self.force_trigger_key)
                .filter(|_| slot == 0),
        }
    }

    pub fn rebind(
        &self,
        action: KeyAction,
        slot: usize,
        chord: Option<Chord>,
    ) -> Result<Self, String> {
        if let Some(chord) = chord {
            if chord.win {
                return Err("Windows keys are reserved by Windows. Pick another key.".into());
            }
            chord.ensure_safe(&chord.to_string())?;
        }
        let mut next = self.clone();
        match (action, chord) {
            (KeyAction::AcceptAll, chord) => replace_at(&mut next.accept_keys, slot, chord),
            (KeyAction::AcceptWord, chord) => replace_at(&mut next.accept_word_keys, slot, chord),
            (KeyAction::Trigger, Some(chord)) => {
                next.force_trigger_key = chord;
                next.force_trigger = true;
            }
            (KeyAction::Trigger, None) => next.force_trigger = false,
            (KeyAction::Dismiss, _) => return Err("Esc always hides the suggestion.".into()),
        }
        next.validate()
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
