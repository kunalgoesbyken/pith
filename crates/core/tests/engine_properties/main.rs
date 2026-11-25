#[path = "../common/mod.rs"]
mod common;
mod harness;
mod pools;

use harness::{Harness, Stats};

const SEEDS: u64 = 300;
const STEPS: usize = 300;

#[test]
fn engine_invariants_hold_over_random_event_sequences() {
    let mut totals = Stats::default();
    for seed in 0..SEEDS {
        let mut harness = Harness::new(seed);
        harness.publish();
        for _ in 0..STEPS {
            harness.run_step();
        }
        totals.add(&harness.stats);
    }
    assert!(totals.shows > 1500, "{}", totals.shows);
    assert!(totals.inserts > 300, "{}", totals.inserts);
    assert!(totals.starts > 2000, "{}", totals.starts);
    assert!(totals.cancels > 250, "{}", totals.cancels);
    assert!(totals.stale > 5000, "{}", totals.stale);
    assert!(totals.dismissals > 1000, "{}", totals.dismissals);
    assert!(totals.checks > 200, "{}", totals.checks);
    assert!(totals.offers > 30, "{}", totals.offers);
    assert!(totals.replaces > 5, "{}", totals.replaces);
}
