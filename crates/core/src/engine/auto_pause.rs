use std::time::Duration;

use super::{Action, Engine, Phase};

pub(super) const READ_TIME: Duration = Duration::from_millis(500);

impl Engine {
    pub(super) fn was_readable(&self) -> bool {
        matches!(&self.phase, Phase::Showing(shown) if shown.shown_at.is_some_and(|at| at.elapsed() >= READ_TIME))
    }

    pub(super) fn unengaged_app(&self) -> Option<String> {
        let field = self.field.as_ref()?;
        matches!(&self.phase, Phase::Showing(shown) if !shown.engaged).then(|| field.app.clone())
    }

    pub(super) fn note_ignored(&mut self, app: &str, out: &mut Vec<Action>) {
        if self.ignores.ignored(app, self.config.auto_pause_after) {
            out.push(Action::AutoPaused(self.ignores.paused()));
        }
    }

    pub(super) fn on_resume(&mut self, app: &str, out: &mut Vec<Action>) {
        if self.ignores.resume(app) {
            out.push(Action::AutoPaused(self.ignores.paused()));
            self.schedule(out);
        }
    }
}
