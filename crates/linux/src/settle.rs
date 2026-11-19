use std::time::{Duration, Instant};

use crate::reading::Reading;

const SETTLE_RETRIES: usize = 6;
const SETTLE_DELAY: Duration = Duration::from_millis(60);

pub(crate) struct Settling {
    left: usize,
    pub(crate) due: Option<Instant>,
}

impl Settling {
    pub(crate) fn new() -> Self {
        Self {
            left: SETTLE_RETRIES,
            due: None,
        }
    }

    pub(crate) fn observe(&mut self, reading: &Reading, changed: bool, loss_pending: bool) {
        if changed {
            self.left = SETTLE_RETRIES;
        }
        let unplaced = matches!(reading, Reading::Field(field) if field.placement.is_none());
        self.due = ((unplaced || loss_pending) && self.left > 0).then(|| {
            self.left -= 1;
            Instant::now() + SETTLE_DELAY
        });
    }
}
