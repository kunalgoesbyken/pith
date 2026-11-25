mod accept;
mod auto_pause;
mod completion;
mod correction;
mod dismissal;
mod events;
mod ignore;
mod replacing;
mod show;
#[cfg(test)]
mod tests;
mod trigger;
mod typing;

use std::sync::Arc;

use crate::context::field::FieldKey;
use crate::session::stats::Counts;
use crate::style::Harvest;
use crate::text::typo::{Declined, Finished};
use crate::{Config, FieldSnapshot, StyleMemory};
use correction::Offer;
use dismissal::Dismissal;
pub use events::{Action, Event};
use ignore::IgnoreTracker;
use replacing::Replacing;
pub use replacing::SelectOutcome;
use show::Shown;

enum Phase {
    Idle,
    Requested,
    Checking(Finished),
    Offering(Offer),
    Showing(Shown),
}

pub struct Engine {
    config: Config,
    field: Option<FieldSnapshot>,
    generation: u64,
    phase: Phase,
    dismissed: Option<Dismissal>,
    crowded_at: Option<(FieldKey, String)>,
    armed: bool,
    ignores: IgnoreTracker,
    forced: Option<FieldKey>,
    unlanded: Vec<String>,
    replacing: Option<Replacing>,
    replaced: u64,
    declined: Declined,
    counts: Counts,
    style: Arc<StyleMemory>,
    harvest: Harvest,
}

impl Engine {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            field: None,
            generation: 0,
            phase: Phase::Idle,
            dismissed: None,
            crowded_at: None,
            armed: false,
            ignores: IgnoreTracker::default(),
            forced: None,
            unlanded: Vec::new(),
            replacing: None,
            replaced: 0,
            declined: Declined::default(),
            counts: Counts::default(),
            style: Arc::default(),
            harvest: Harvest::default(),
        }
    }

    pub fn counts(&self) -> Counts {
        self.counts
    }

    pub fn take_counts(&mut self) -> Counts {
        std::mem::take(&mut self.counts)
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn suggestion(&self) -> Option<&str> {
        match &self.phase {
            Phase::Showing(shown) => Some(&shown.text),
            Phase::Idle | Phase::Requested | Phase::Checking(_) | Phase::Offering(_) => None,
        }
    }

    pub fn is_visible(&self) -> bool {
        matches!(self.phase, Phase::Showing(_) | Phase::Offering(_))
    }

    fn is_idle(&self) -> bool {
        matches!(self.phase, Phase::Idle)
    }

    pub fn handle(&mut self, event: Event) -> Vec<Action> {
        let mut out = Vec::new();
        match event {
            Event::Field(None) => self.on_field_lost(&mut out),
            Event::Field(Some(field)) => self.on_field(field, &mut out),
            Event::DebounceElapsed(generation) => self.on_debounce(generation, &mut out),
            Event::Completed { generation, result } => {
                self.on_completed(generation, result, &mut out)
            }
            Event::AcceptAll => self.on_accept(false, &mut out),
            Event::AcceptWord => self.on_accept(true, &mut out),
            Event::Dismiss => self.on_dismiss(&mut out),
            Event::Drawn(drawn) => self.on_drawn(drawn, &mut out),
            Event::Spelled {
                generation,
                spelling,
            } => self.on_spelled(generation, spelling, &mut out),
            Event::Selected { id, outcome } => self.on_selected(id, outcome, &mut out),
            Event::ReplaceTimedOut(id) => self.on_replace_timeout(id, &mut out),
            Event::Trigger => self.on_trigger(&mut out),
            Event::Resume(app) => self.on_resume(&app, &mut out),
            Event::Style(memory) => self.style = memory,
            Event::SetConfig(config) => {
                self.config = config;
                self.harvest.clear();
                self.reset(&mut out);
                self.schedule(&mut out);
            }
        }
        out
    }
}
