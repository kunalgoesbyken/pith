mod invariants;
mod steps;

use pith_core::{Config, Correction, Engine, FieldSnapshot};

use crate::common::Rng;
use crate::pools::{LANGUAGE, placement};

pub(crate) struct Harness {
    engine: Engine,
    rng: Rng,
    config: Config,
    world: Option<FieldSnapshot>,
    sent: Option<FieldSnapshot>,
    scheduled: Vec<u64>,
    sched: Option<u64>,
    last_sched: u64,
    last_start: u64,
    flight: Option<u64>,
    live: Option<u64>,
    shown: Option<String>,
    lagging: bool,
    offer: Option<Correction>,
    selecting: Option<(u64, Correction)>,
    selecting_key: Option<(u64, String, String)>,
    selecting_aborted: bool,
    declined: Vec<String>,
    checking: Option<(u64, String)>,
    armed: bool,
    dismissed: Option<(u64, String)>,
    trace: Vec<String>,
    pub(crate) stats: Stats,
}

#[derive(Default)]
pub(crate) struct Stats {
    pub(crate) shows: usize,
    pub(crate) inserts: usize,
    pub(crate) starts: usize,
    pub(crate) cancels: usize,
    pub(crate) stale: usize,
    pub(crate) dismissals: usize,
    pub(crate) checks: usize,
    pub(crate) offers: usize,
    pub(crate) replaces: usize,
}

impl Stats {
    pub(crate) fn add(&mut self, other: &Stats) {
        self.shows += other.shows;
        self.inserts += other.inserts;
        self.starts += other.starts;
        self.cancels += other.cancels;
        self.stale += other.stale;
        self.dismissals += other.dismissals;
        self.checks += other.checks;
        self.offers += other.offers;
        self.replaces += other.replaces;
    }
}

impl Harness {
    pub(crate) fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let config = Config {
            max_words: 1 + rng.below(8),
            min_chars_before: rng.below(6),
            ..Config::default()
        };
        let world = FieldSnapshot {
            field_id: 1,
            app: "notepad".into(),
            before: "Please sign the".into(),
            placement: Some(placement(&mut rng)),
            language: Some(LANGUAGE.into()),
            ..Default::default()
        };
        Self {
            engine: Engine::new(config.clone()),
            rng,
            config,
            world: Some(world),
            sent: None,
            scheduled: Vec::new(),
            sched: None,
            last_sched: 0,
            last_start: 0,
            flight: None,
            live: None,
            shown: None,
            lagging: false,
            offer: None,
            selecting: None,
            selecting_key: None,
            selecting_aborted: false,
            declined: Vec::new(),
            checking: None,
            armed: false,
            dismissed: None,
            trace: Vec::new(),
            stats: Stats::default(),
        }
    }

    fn fail(&self, message: &str) -> ! {
        let tail = self.trace.len().saturating_sub(12);
        panic!("{message}\nlast events:\n{}", self.trace[tail..].join("\n"));
    }

    fn expect(&self, ok: bool, message: &str) {
        if !ok {
            self.fail(message);
        }
    }
}
