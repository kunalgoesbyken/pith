use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::config::stored::{set_aside, write_atomically};

const CHARS_PER_WORD: u64 = 5;
const SECONDS_PER_MINUTE: u64 = 60;
const HINTED_ACCEPTS: u64 = 3;
pub const HISTORY_DAYS: usize = 14;
const DAYS_PER_ERA: i64 = 146_097;
const EPOCH_SHIFT: i64 = 719_468;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Day(u32);

impl Day {
    pub fn from_ymd(year: u16, month: u8, day: u8) -> Self {
        Self(u32::from(year) * 10_000 + u32::from(month) * 100 + u32::from(day))
    }

    fn ordinal(self) -> i64 {
        let year = i64::from(self.0 / 10_000);
        let month = i64::from(self.0 / 100 % 100);
        let day = i64::from(self.0 % 100);
        let year = year - i64::from(month <= 2);
        let era = year.div_euclid(400);
        let year_of_era = year - era * 400;
        let shifted_month = (month + 9) % 12;
        let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * DAYS_PER_ERA + day_of_era - EPOCH_SHIFT
    }

    fn days_since(self, earlier: Day) -> i64 {
        self.ordinal() - earlier.ordinal()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct DayWords {
    day: Day,
    words: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Counts {
    pub shown: u64,
    pub accepted: u64,
    pub accepted_words: u64,
    pub used: u64,
    pub words: u64,
    pub chars: u64,
}

impl Counts {
    pub fn record_shown(&mut self) {
        self.shown = self.shown.saturating_add(1);
    }

    pub fn record_used(&mut self) {
        self.used = self.used.saturating_add(1);
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

    pub fn kept_percent(&self) -> Option<u64> {
        (self.shown > 0).then(|| (self.used.saturating_mul(100) / self.shown).min(100))
    }

    pub fn needs_key_hint(&self) -> bool {
        self.accepted.saturating_add(self.accepted_words) < HINTED_ACCEPTS
    }

    pub fn add(&mut self, other: Counts) {
        self.shown = self.shown.saturating_add(other.shown);
        self.accepted = self.accepted.saturating_add(other.accepted);
        self.accepted_words = self.accepted_words.saturating_add(other.accepted_words);
        self.used = self.used.saturating_add(other.used);
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    history: Vec<DayWords>,
}

impl Stats {
    pub fn record(&mut self, counts: Counts, day: Day) {
        if day != self.day {
            self.close_day(day);
        }
        self.today.add(counts);
        self.total.add(counts);
    }

    fn close_day(&mut self, next: Day) {
        if self.today.words > 0 {
            self.history.push(DayWords {
                day: self.day,
                words: self.today.words,
            });
        }
        self.history
            .retain(|entry| (1..HISTORY_DAYS as i64).contains(&next.days_since(entry.day)));
        self.day = next;
        self.today = Counts::default();
    }

    pub fn recent_words(&self, today: Day) -> [u64; HISTORY_DAYS] {
        let mut words = [0; HISTORY_DAYS];
        let open = DayWords {
            day: self.day,
            words: self.today.words,
        };
        for entry in self.history.iter().chain([&open]) {
            let age = today.days_since(entry.day);
            if (0..HISTORY_DAYS as i64).contains(&age) {
                words[HISTORY_DAYS - 1 - age as usize] = entry.words;
            }
        }
        words
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

    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error.to_string()),
        };
        match std::str::from_utf8(&bytes)
            .ok()
            .and_then(|source| toml_edit::de::from_str(source).ok())
        {
            Some(stats) => Ok(stats),
            None => set_aside(path).map(|()| Self::default()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = toml_edit::ser::to_string_pretty(self).map_err(|error| error.to_string())?;
        write_atomically(path, &text).map_err(|error| error.to_string())
    }
}

enum Backing {
    Memory,
    Unread(PathBuf),
    File(PathBuf),
}

pub struct Ledger {
    stats: Stats,
    backing: Backing,
    dirty: bool,
    saved_at: Instant,
}

impl Ledger {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            stats: Stats::default(),
            backing: path.map_or(Backing::Memory, Backing::Unread),
            dirty: false,
            saved_at: Instant::now(),
        }
    }

    pub fn load(&mut self) -> Result<(), String> {
        let Backing::Unread(path) = &self.backing else {
            return Ok(());
        };
        self.stats = Stats::load(path)?;
        self.backing = Backing::File(path.clone());
        Ok(())
    }

    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    pub fn record(&mut self, counts: Counts, day: Day) -> bool {
        if counts.is_empty() {
            return false;
        }
        self.stats.record(counts, day);
        self.dirty = true;
        true
    }

    pub fn flush_every(&mut self, interval: Duration) -> Result<(), String> {
        if self.saved_at.elapsed() < interval {
            return Ok(());
        }
        self.flush()
    }

    pub fn flush(&mut self) -> Result<(), String> {
        let Backing::File(path) = &self.backing else {
            return Ok(());
        };
        if !self.dirty {
            return Ok(());
        }
        self.saved_at = Instant::now();
        self.stats.save(path)?;
        self.dirty = false;
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), String> {
        self.stats = Stats::default();
        if let Backing::Unread(path) = &self.backing {
            self.backing = Backing::File(path.clone());
        }
        self.dirty = true;
        self.flush()
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
        stats.record(counts(1, 0, 0, 0), MONDAY);
        assert_eq!(stats.today(MONDAY), counts(1, 0, 0, 0));
        assert_eq!(stats.total().shown, 9);
    }

    #[test]
    fn saved_stats_load_back_and_hold_only_numbers_and_unknown_fields_are_ignored() {
        let dir = scratch("round-trip");
        let path = dir.join("stats.toml");
        let mut stats = Stats::default();
        stats.record(counts(3, 2, 5, 31), MONDAY);
        stats.save(&path).unwrap();
        assert_eq!(Stats::load(&path), Ok(stats));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.lines().all(|line| {
            let (_, value) = line.split_once(" = ").unwrap_or(("", "0"));
            line.is_empty() || line.starts_with('[') || value.parse::<u64>().is_ok()
        }));
        std::fs::write(&path, "future = 1\n[total]\nshown = 7\n").unwrap();
        assert_eq!(Stats::load(&path).unwrap().total().shown, 7);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_corrupt_files_reset_to_zero_and_keep_the_corrupt_copy() {
        let dir = scratch("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(Stats::load(&dir.join("absent.toml")), Ok(Stats::default()));
        let path = dir.join("stats.toml");
        let kept = dir.join("stats.toml.bad");
        for broken in [
            "not toml [",
            "day = \"yesterday\"",
            "[today]\nshown = -4",
            "[total]\nshown = 99999999999999999999999",
            "\u{0}\u{1}",
        ] {
            std::fs::write(&path, broken).unwrap();
            assert_eq!(Stats::load(&path), Ok(Stats::default()), "{broken:?}");
            assert!(!path.exists(), "{broken:?}");
            assert_eq!(std::fs::read_to_string(&kept).unwrap(), broken);
        }
        std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(Stats::load(&path), Ok(Stats::default()));
        assert_eq!(std::fs::read(&kept).unwrap(), [0xff, 0xfe, 0x00]);
        std::fs::write(&path, "").unwrap();
        assert_eq!(Stats::load(&path), Ok(Stats::default()));
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unreadable_file_is_an_error_so_it_is_never_overwritten() {
        let dir = scratch("unreadable");
        let path = dir.join("stats.toml");
        std::fs::create_dir_all(&path).unwrap();
        assert!(Stats::load(&path).is_err());
        assert!(path.is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
