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
        assert!(!tracker.ignored("slack", 12));
        assert_eq!(tracker.paused().len(), 1);

        let mut tracker = IgnoreTracker::default();
        for _ in 0..5 {
            assert!(!ignore_times(&mut tracker, "slack", 11, 12));
            tracker.accepted("slack");
        }
        assert!(!tracker.is_paused("slack"));
        assert!(!ignore_times(&mut tracker, "slack", 11, 12));
        assert!(tracker.ignored("slack", 12));

        let mut tracker = IgnoreTracker::default();
        assert!(!ignore_times(&mut tracker, "slack", 1000, 0));
        assert!(!tracker.is_paused("slack"));

        let mut tracker = IgnoreTracker::default();
        assert!(tracker.ignored("slack", 1));
        assert!(tracker.ignored("\u{c9}diteur", 1));
        assert!(tracker.is_paused("\u{e9}diteur"));
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
        assert_eq!(tracker.paused(), vec!["SLACK".to_string()]);
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
}
