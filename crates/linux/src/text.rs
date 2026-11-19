const ASTRAL_FILLER: char = '\u{feff}';
const FIRST_ASTRAL: char = '\u{10000}';

pub(crate) fn without_astral_fillers(text: &str) -> String {
    let mut previous = None;
    text.chars()
        .filter(|&c| {
            let filler = c == ASTRAL_FILLER && previous.is_some_and(|p| p >= FIRST_ASTRAL);
            previous = Some(c);
            !filler
        })
        .collect()
}
