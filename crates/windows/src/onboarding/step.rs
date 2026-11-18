const MEGABYTE: u64 = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Preparing,
    Downloading { done: u64, total: u64 },
    Loading,
    Practice,
    Failed(String),
    Finished,
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Progress(u64, u64),
    Loading,
    Ready,
    Failed(String),
    Done,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Busy,
    Ready,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub status: String,
    pub glyph: Glyph,
    pub permille: Option<u32>,
    pub practice_enabled: bool,
    pub done_enabled: bool,
}

fn megabytes(bytes: u64) -> u64 {
    bytes.saturating_add(MEGABYTE / 2) / MEGABYTE
}

impl Step {
    pub fn next(self, event: Event) -> Step {
        match (self, event) {
            (step @ (Step::Finished | Step::Closed), _) => step,
            (_, Event::Close) => Step::Closed,
            (Step::Practice, Event::Done) => Step::Finished,
            (Step::Failed(_), Event::Done) => Step::Closed,
            (step @ Step::Practice, _) | (step @ Step::Failed(_), _) => step,
            (_, Event::Ready) => Step::Practice,
            (_, Event::Failed(error)) => Step::Failed(error),
            (_, Event::Progress(done, total)) if done >= total && total > 0 => Step::Loading,
            (_, Event::Progress(done, total)) => Step::Downloading {
                done: done.min(total),
                total,
            },
            (Step::Preparing, Event::Loading) => Step::Loading,
            (step, _) => step,
        }
    }

    pub fn is_over(&self) -> bool {
        matches!(self, Step::Finished | Step::Closed)
    }

    pub fn view(&self) -> View {
        let (status, glyph, permille) = match self {
            Step::Preparing => ("Starting Pith".to_string(), Glyph::Busy, None),
            Step::Downloading { done, total } => (
                format!(
                    "Downloading model \u{b7} {} / {} MB",
                    megabytes(*done),
                    megabytes(*total)
                ),
                Glyph::Busy,
                Some(((*done as u128 * 1000) / (*total).max(1) as u128) as u32),
            ),
            Step::Loading => ("Loading the model".to_string(), Glyph::Busy, None),
            Step::Practice | Step::Finished | Step::Closed => {
                ("Ready".to_string(), Glyph::Ready, None)
            }
            Step::Failed(error) => (
                format!("Pith could not start: {error}"),
                Glyph::Failed,
                None,
            ),
        };
        View {
            status,
            glyph,
            permille,
            practice_enabled: matches!(self, Step::Practice),
            done_enabled: matches!(self, Step::Practice | Step::Failed(_)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn downloading(done: u64, total: u64) -> Step {
        Step::Downloading { done, total }
    }

    fn failed(message: &str) -> Step {
        Step::Failed(message.into())
    }

    #[test]
    fn events_move_the_flow_forward_and_late_ones_are_ignored() {
        let rows = [
            (
                Step::Preparing,
                Event::Progress(0, 397),
                downloading(0, 397),
            ),
            (
                downloading(0, 397),
                Event::Progress(100, 397),
                downloading(100, 397),
            ),
            (downloading(100, 397), Event::Loading, downloading(100, 397)),
            (
                downloading(100, 397),
                Event::Progress(397, 397),
                Step::Loading,
            ),
            (Step::Loading, Event::Loading, Step::Loading),
            (Step::Loading, Event::Ready, Step::Practice),
            (Step::Practice, Event::Done, Step::Finished),
            (Step::Preparing, Event::Loading, Step::Loading),
            (Step::Preparing, Event::Ready, Step::Practice),
            (Step::Preparing, Event::Progress(0, 0), downloading(0, 0)),
            (Step::Preparing, Event::Progress(9, 5), Step::Loading),
            (Step::Preparing, Event::Done, Step::Preparing),
            (downloading(1, 2), Event::Done, downloading(1, 2)),
            (Step::Loading, Event::Done, Step::Loading),
            (Step::Practice, Event::Progress(1, 2), Step::Practice),
            (Step::Practice, Event::Loading, Step::Practice),
            (Step::Practice, Event::Failed("late".into()), Step::Practice),
            (
                Step::Preparing,
                Event::Failed("offline".into()),
                failed("offline"),
            ),
            (failed("offline"), Event::Ready, failed("offline")),
            (failed("x"), Event::Done, Step::Closed),
        ];
        for row in rows {
            let (step, event, expected) = row.clone();
            assert_eq!(step.next(event), expected, "{row:?}");
        }
    }

    #[test]
    fn closing_works_from_every_live_step_and_the_ends_are_terminal() {
        for step in [
            Step::Preparing,
            downloading(1, 2),
            Step::Loading,
            Step::Practice,
            failed("x"),
        ] {
            assert_eq!(step.clone().next(Event::Close), Step::Closed, "{step:?}");
        }
        for end in [Step::Finished, Step::Closed] {
            for event in [
                Event::Ready,
                Event::Done,
                Event::Loading,
                Event::Close,
                Event::Progress(1, 2),
                Event::Failed("x".into()),
            ] {
                assert_eq!(end.clone().next(event.clone()), end, "{end:?} {event:?}");
            }
        }
    }

    #[test]
    fn view_reports_size_and_gates_controls() {
        let view = downloading(396_704_960 / 2, 396_704_960).view();
        assert_eq!(view.status, "Downloading model \u{b7} 198 / 397 MB");
        assert_eq!(view.permille, Some(500));
        assert!(!view.practice_enabled && !view.done_enabled);
        let practice = Step::Practice.view();
        assert_eq!(practice.status, "Ready");
        assert!(practice.practice_enabled && practice.done_enabled);
        let failed = Step::Failed("offline".into()).view();
        assert_eq!(failed.status, "Pith could not start: offline");
        assert!(!failed.practice_enabled && failed.done_enabled);
    }

    #[test]
    fn view_never_overflows_and_a_zero_total_reads_as_zero() {
        let rows = [
            (downloading(0, 0), Some(0)),
            (downloading(u64::MAX - 1, u64::MAX), Some(999)),
        ];
        for row in rows {
            let (step, permille) = row.clone();
            assert_eq!(step.view().permille, permille, "{row:?}");
        }
    }
}
