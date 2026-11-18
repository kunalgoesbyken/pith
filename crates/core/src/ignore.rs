use std::collections::HashMap;

#[derive(Default)]
pub struct IgnoreTracker {
    counts: HashMap<String, u32>,
    paused: Vec<(String, String)>,
}

fn key(app: &str) -> String {
    app.to_lowercase()
}

impl IgnoreTracker {
    pub fn ignored(&mut self, app: &str, limit: u32) -> bool {
        let key = key(app);
        if limit == 0 || self.paused.iter().any(|(paused, _)| *paused == key) {
            return false;
        }
        let count = self.counts.entry(key.clone()).or_default();
        *count += 1;
        if *count < limit {
            return false;
        }
        self.counts.remove(&key);
        self.paused.push((key, app.to_string()));
        true
    }

    pub fn accepted(&mut self, app: &str) {
        self.counts.remove(&key(app));
    }

    pub fn resume(&mut self, app: &str) -> bool {
        let key = key(app);
        self.counts.remove(&key);
        let before = self.paused.len();
        self.paused.retain(|(paused, _)| *paused != key);
        self.paused.len() != before
    }

    pub fn is_paused(&self, app: &str) -> bool {
        let key = key(app);
        self.paused.iter().any(|(paused, _)| *paused == key)
    }

    pub fn paused(&self) -> Vec<String> {
        self.paused.iter().map(|(_, name)| name.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ignore_times(tracker: &mut IgnoreTracker, app: &str, times: u32, limit: u32) -> bool {
        (0..times).fold(false, |paused, _| tracker.ignored(app, limit) || paused)
    }

    #[test]
    fn pauses_exactly_at_the_limit() {
        let mut tracker = IgnoreTracker::default();
        assert!(!ignore_times(&mut tracker, "slack", 11, 12));
        assert!(!tracker.is_paused("slack"));
        assert!(tracker.ignored("slack", 12));
        assert!(tracker.is_paused("slack"));
        assert_eq!(tracker.paused(), vec!["slack".to_string()]);
    }

    #[test]
    fn accept_resets_the_streak() {
        let mut tracker = IgnoreTracker::default();
        for _ in 0..5 {
            assert!(!ignore_times(&mut tracker, "slack", 11, 12));
            tracker.accepted("slack");
        }
        assert!(!tracker.is_paused("slack"));
        assert!(!ignore_times(&mut tracker, "slack", 11, 12));
        assert!(tracker.ignored("slack", 12));
    }

    #[test]
    fn apps_count_separately_and_ignore_case() {
        let mut tracker = IgnoreTracker::default();
        assert!(!ignore_times(&mut tracker, "Slack", 5, 6));
        assert!(!ignore_times(&mut tracker, "notepad", 5, 6));
        assert!(tracker.ignored("SLACK", 6));
        assert!(tracker.is_paused("slack"));
        assert!(!tracker.is_paused("notepad"));
        tracker.accepted("notepad");
        assert!(!ignore_times(&mut tracker, "notepad", 5, 6));
        assert_eq!(tracker.paused(), vec!["Slack".to_string()]);
    }

    #[test]
    fn resume_clears_pause_and_counter() {
        let mut tracker = IgnoreTracker::default();
        assert!(ignore_times(&mut tracker, "slack", 3, 3));
        assert!(tracker.resume("Slack"));
        assert!(!tracker.is_paused("slack"));
        assert!(tracker.paused().is_empty());
        assert!(!tracker.resume("slack"));
        assert!(!ignore_times(&mut tracker, "slack", 2, 3));
        assert!(tracker.ignored("slack", 3));
    }

    #[test]
    fn paused_app_reports_the_pause_once() {
        let mut tracker = IgnoreTracker::default();
        assert!(ignore_times(&mut tracker, "slack", 3, 3));
        assert!(!tracker.ignored("slack", 3));
        assert_eq!(tracker.paused().len(), 1);
    }

    #[test]
    fn zero_limit_never_pauses() {
        let mut tracker = IgnoreTracker::default();
        assert!(!ignore_times(&mut tracker, "slack", 1000, 0));
        assert!(!tracker.is_paused("slack"));
    }

    #[test]
    fn limit_of_one_pauses_on_the_first_ignore() {
        let mut tracker = IgnoreTracker::default();
        assert!(tracker.ignored("slack", 1));
    }

    #[test]
    fn non_latin_and_empty_names_are_ordinary_keys() {
        let mut tracker = IgnoreTracker::default();
        for app in ["\u{65e5}\u{672c}", "\u{c9}diteur", "", "__proto__"] {
            assert!(tracker.ignored(app, 1), "{app:?}");
            assert!(tracker.is_paused(app), "{app:?}");
        }
        assert!(tracker.is_paused("\u{e9}diteur"));
        assert_eq!(tracker.paused().len(), 4);
    }
}
