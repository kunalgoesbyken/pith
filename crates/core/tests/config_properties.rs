mod common;

use common::{Rng, text};
use pith_core::{Config, FieldSnapshot, SkipReason, app_name, check};

const CASES: usize = 4000;

const APP_PIECES: &[&str] = &[
    "foo",
    "Bar",
    ".",
    "exe",
    ".exe",
    ".EXE",
    " ",
    "\t",
    "\\",
    "/",
    "C:",
    "Program Files",
    "é",
    "日本",
    "",
    "\u{a0}",
];

fn app(rng: &mut Rng) -> String {
    (0..rng.below(7)).map(|_| *rng.pick(APP_PIECES)).collect()
}

#[test]
fn app_name_is_idempotent_and_stable_under_blocking() {
    let mut rng = Rng::new(10);
    for _ in 0..CASES {
        let raw = app(&mut rng);
        let once = app_name(&raw);
        assert_eq!(app_name(&once), once, "{raw:?}");
        let config = Config {
            blocked_apps: vec![raw.clone()],
            ..Config::default()
        };
        if !once.is_empty() {
            assert!(config.is_blocked(&raw), "{raw:?}");
            assert!(config.is_blocked(&once), "{raw:?} {once:?}");
        }
    }
}

#[test]
fn policy_is_total_and_ordered_for_any_snapshot() {
    let mut rng = Rng::new(11);
    for _ in 0..CASES {
        let field = FieldSnapshot {
            app: "notepad".into(),
            before: text(&mut rng, 20),
            after: text(&mut rng, 6),
            has_selection: rng.chance(10),
            is_password: rng.chance(10),
            composing: rng.chance(10),
            ..Default::default()
        };
        let config = Config {
            min_chars_before: rng.below(8),
            ..Config::default()
        };
        let verdict = check(&field, &config);
        if field.is_password {
            assert_eq!(verdict, Err(SkipReason::Password), "{field:?}");
        }
        if verdict.is_ok() {
            let line = field.before.rsplit(['\n', '\r']).next().unwrap_or_default();
            assert!(
                line.trim().chars().count() >= config.min_chars_before,
                "{field:?}"
            );
            let rest = field.after.split(['\n', '\r']).next().unwrap_or_default();
            assert!(rest.trim().is_empty(), "{field:?}");
        }
        assert_eq!(check(&field, &config), verdict);
    }
}

#[test]
fn config_round_trips_through_toml() {
    let mut rng = Rng::new(12);
    for _ in 0..500 {
        let config = Config {
            enabled: rng.chance(50),
            threads: rng.below(16),
            context_tokens: 1024 + rng.below(8192) as u32,
            idle_unload_secs: rng.below(10_000) as u64,
            debounce_ms: rng.below(5_001) as u64,
            max_tokens: 1 + rng.below(512),
            max_words: 1 + rng.below(32),
            min_token_logprob: -(rng.below(1000) as f32) / 7.0,
            min_mean_logprob: -(rng.below(1000) as f32) / 3.0,
            context_chars: 1 + rng.below(4000),
            min_chars_before: rng.below(20),
            blocked_apps: (0..rng.below(4)).map(|_| text(&mut rng, 3)).collect(),
            remote_url: rng
                .chance(30)
                .then(|| format!("https://example.com/{}", rng.below(100))),
            remote_model: rng.chance(30).then(|| text(&mut rng, 3)),
            ..Config::default()
        };
        let reparsed = Config::from_toml(&config.to_toml());
        assert_eq!(reparsed, Ok(config.clone()), "{config:?}");
    }
}
