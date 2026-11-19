use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Due {
    Debounce(u64),
    KeyDeadline,
    KeyPoll,
    PauseCheck,
}

#[derive(Default)]
pub(super) struct Timers {
    debounce: Option<(Instant, u64)>,
    key_deadline: Option<Instant>,
    key_poll: Option<Instant>,
    pause_check: Option<Instant>,
}

impl Timers {
    pub(super) fn schedule_debounce(&mut self, delay: Duration, generation: u64) {
        self.debounce = Some((Instant::now() + delay, generation));
    }

    pub(super) fn start_key_deadline(&mut self, limit: Duration) {
        self.key_deadline
            .get_or_insert_with(|| Instant::now() + limit);
    }

    pub(super) fn start_key_poll(&mut self, interval: Duration) {
        self.key_poll
            .get_or_insert_with(|| Instant::now() + interval);
    }

    pub(super) fn start_pause_check(&mut self, interval: Duration) {
        self.pause_check
            .get_or_insert_with(|| Instant::now() + interval);
    }

    pub(super) fn clear_key_deadline(&mut self) {
        self.key_deadline = None;
    }

    pub(super) fn next(&self) -> Option<Instant> {
        [
            self.debounce.map(|(at, _)| at),
            self.key_deadline,
            self.key_poll,
            self.pause_check,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub(super) fn take_due(&mut self, now: Instant) -> Vec<Due> {
        let mut due = Vec::new();
        if let Some((at, generation)) = self.debounce
            && at <= now
        {
            self.debounce = None;
            due.push(Due::Debounce(generation));
        }
        if self.key_deadline.is_some_and(|at| at <= now) {
            self.key_deadline = None;
            due.push(Due::KeyDeadline);
        }
        if self.key_poll.is_some_and(|at| at <= now) {
            self.key_poll = None;
            due.push(Due::KeyPoll);
        }
        if self.pause_check.is_some_and(|at| at <= now) {
            self.pause_check = None;
            due.push(Due::PauseCheck);
        }
        due
    }
}
