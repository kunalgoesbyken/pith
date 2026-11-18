use std::path::Path;

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::stored::{read, write_atomically};

const CHARS_PER_WORD: u64 = 5;
const SECONDS_PER_MINUTE: u64 = 60;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Day(u32);

impl Day {
    pub fn from_ymd(year: u16, month: u8, day: u8) -> Self {
        Self(u32::from(year) * 10_000 + u32::from(month) * 100 + u32::from(day))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Counts {
    pub shown: u64,
    pub accepted: u64,
    pub accepted_words: u64,
    pub words: u64,
    pub chars: u64,
}

impl Counts {
    pub fn record_shown(&mut self) {
        self.shown = self.shown.saturating_add(1);
    }

    pub fn record_accept(&mut self, text: &str, by_word: bool) {
        let field = if by_word {
            &mut self.accepted_words
        } else {
            &mut self.accepted
        };
        *field = field.saturating_add(1);
        self.words = self
            .words
            .saturating_add(text.unicode_words().count() as u64);
        self.chars = self.chars.saturating_add(text.chars().count() as u64);
    }

    pub fn add(&mut self, other: Counts) {
        self.shown = self.shown.saturating_add(other.shown);
        self.accepted = self.accepted.saturating_add(other.accepted);
        self.accepted_words = self.accepted_words.saturating_add(other.accepted_words);
        self.words = self.words.saturating_add(other.words);
        self.chars = self.chars.saturating_add(other.chars);
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn seconds_saved(&self, typing_wpm: u32) -> u64 {
        let chars_per_minute = u64::from(typing_wpm.max(1)) * CHARS_PER_WORD;
        self.chars.saturating_mul(SECONDS_PER_MINUTE) / chars_per_minute
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stats {
    day: Day,
    today: Counts,
    total: Counts,
}

impl Stats {
    pub fn record(&mut self, counts: Counts, day: Day) {
        if day != self.day {
            self.day = day;
            self.today = Counts::default();
        }
        self.today.add(counts);
        self.total.add(counts);
    }

    pub fn today(&self, day: Day) -> Counts {
        if day == self.day {
            self.today
        } else {
            Counts::default()
        }
    }

    pub fn total(&self) -> Counts {
        self.total
    }

    pub fn load(path: &Path) -> Self {
        read(path)
            .ok()
            .flatten()
            .and_then(|source| toml_edit::de::from_str(&source).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = toml_edit::ser::to_string_pretty(self).map_err(|error| error.to_string())?;
        write_atomically(path, &text).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONDAY: Day = Day(20261116);
    const TUESDAY: Day = Day(20261117);

    fn counts(shown: u64, accepted: u64, words: u64, chars: u64) -> Counts {
        Counts {
            shown,
            accepted,
            words,
            chars,
            ..Counts::default()
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pith-stats-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn days_order_by_calendar_date() {
        assert!(Day::from_ymd(2026, 12, 31) < Day::from_ymd(2027, 1, 1));
        assert!(Day::from_ymd(2026, 2, 9) < Day::from_ymd(2026, 2, 10));
        assert_eq!(Day::from_ymd(2026, 11, 16), MONDAY);
    }

    #[test]
    fn accepts_count_words_and_chars_by_unicode_rules() {
        let mut counts = Counts::default();
        counts.record_accept("the quick fox", false);
        counts.record_accept(" jumps", true);
        counts.record_accept("na\u{ef}ve caf\u{e9}, d\u{e9}j\u{e0}-vu", true);
        counts.record_accept("", false);
        assert_eq!(counts.accepted, 2);
        assert_eq!(counts.accepted_words, 2);
        assert_eq!(counts.words, 3 + 1 + 4);
        assert_eq!(counts.chars, 13 + 6 + 19);
    }

    #[test]
    fn counters_saturate_instead_of_overflowing() {
        let mut full = Counts {
            shown: u64::MAX,
            accepted: u64::MAX,
            accepted_words: u64::MAX,
            words: u64::MAX,
            chars: u64::MAX,
        };
        full.record_shown();
        full.record_accept("a b", false);
        full.add(full);
        assert_eq!(full.shown, u64::MAX);
        assert_eq!(full.chars, u64::MAX);
        assert_eq!(full.seconds_saved(1), u64::MAX / CHARS_PER_WORD);
    }

    #[test]
    fn time_saved_follows_the_typing_speed() {
        let counts = counts(0, 0, 0, 200);
        assert_eq!(counts.seconds_saved(40), 60);
        assert_eq!(counts.seconds_saved(20), 120);
        assert_eq!(counts.seconds_saved(0), counts.seconds_saved(1));
        assert_eq!(Counts::default().seconds_saved(40), 0);
    }

    #[test]
    fn today_rolls_over_on_a_new_date_and_total_keeps_growing() {
        let mut stats = Stats::default();
        stats.record(counts(2, 1, 3, 10), MONDAY);
        stats.record(counts(1, 0, 0, 0), MONDAY);
        assert_eq!(stats.today(MONDAY), counts(3, 1, 3, 10));
        assert_eq!(stats.today(TUESDAY), Counts::default());
        stats.record(counts(5, 2, 4, 20), TUESDAY);
        assert_eq!(stats.today(TUESDAY), counts(5, 2, 4, 20));
        assert_eq!(stats.total(), counts(8, 3, 7, 30));
    }

    #[test]
    fn a_clock_set_back_still_starts_a_fresh_day() {
        let mut stats = Stats::default();
        stats.record(counts(4, 0, 0, 0), TUESDAY);
        stats.record(counts(1, 0, 0, 0), MONDAY);
        assert_eq!(stats.today(MONDAY), counts(1, 0, 0, 0));
        assert_eq!(stats.total().shown, 5);
    }

    #[test]
    fn saved_stats_load_back_and_hold_only_numbers() {
        let dir = scratch("round-trip");
        let path = dir.join("stats.toml");
        let mut stats = Stats::default();
        stats.record(counts(3, 2, 5, 31), MONDAY);
        stats.save(&path).unwrap();
        assert_eq!(Stats::load(&path), stats);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.lines().all(|line| {
            let (_, value) = line.split_once(" = ").unwrap_or(("", "0"));
            line.is_empty() || line.starts_with('[') || value.parse::<u64>().is_ok()
        }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_corrupt_files_reset_to_zero() {
        let dir = scratch("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(Stats::load(&dir.join("absent.toml")), Stats::default());
        let path = dir.join("stats.toml");
        for broken in [
            "not toml [",
            "day = \"yesterday\"",
            "[today]\nshown = -4",
            "[total]\nshown = 99999999999999999999999",
            "\u{0}\u{1}",
            "",
        ] {
            std::fs::write(&path, broken).unwrap();
            assert_eq!(Stats::load(&path), Stats::default(), "{broken:?}");
        }
        std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(Stats::load(&path), Stats::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_or_missing_fields_do_not_lose_the_known_ones() {
        let dir = scratch("partial");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("stats.toml");
        std::fs::write(&path, "future = 1\n[total]\nshown = 7\n").unwrap();
        assert_eq!(Stats::load(&path).total().shown, 7);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
