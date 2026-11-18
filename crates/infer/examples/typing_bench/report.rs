use pith_core::{Completion, Config, next_word, split_partial, suggestion};

pub const MISS_TOKENS: usize = 16;

pub struct Record {
    pub position: usize,
    pub before: String,
    pub rest: String,
    pub completion: Completion,
    pub ms: f64,
    pub evaluated: usize,
}

pub struct Shown {
    pub text: String,
    pub common: usize,
    pub accepted: usize,
}

impl Shown {
    pub fn exact(&self) -> bool {
        self.common == self.text.chars().count()
    }

    pub fn wrong(&self) -> bool {
        self.common == 0
    }
}

fn accepted_chars(suggestion: &str, rest: &str) -> usize {
    let mut taken = 0;
    loop {
        let word = next_word(&suggestion[taken..]);
        if word.is_empty() || !rest[taken..].starts_with(word) {
            return suggestion[..taken].chars().count();
        }
        taken += word.len();
    }
}

pub fn show(record: &Record, config: &Config) -> Option<Shown> {
    let text = suggestion(&record.completion, &record.before, config)?;
    let common = text
        .chars()
        .zip(record.rest.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let accepted = accepted_chars(&text, &record.rest);
    Some(Shown {
        text,
        common,
        accepted,
    })
}

fn at_word_start(before: &str) -> bool {
    !before.is_empty() && split_partial(before).1.trim().is_empty()
}

fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn ratio(part: usize, total: usize) -> f64 {
    100.0 * part as f64 / total.max(1) as f64
}

#[derive(Default)]
pub struct Stats {
    ms: Vec<f64>,
    evaluated: Vec<f64>,
    tokens: Vec<f64>,
    after_backspace: Vec<f64>,
    pub misses: Vec<(usize, usize)>,
    word_hits: usize,
    word_total: usize,
    saved: usize,
    chars: usize,
    requests: usize,
    shown: usize,
    exact: usize,
    partial: usize,
    wrong: usize,
}

impl Stats {
    pub fn latency_only(&mut self, record: &Record) {
        self.ms.push(record.ms);
        self.evaluated.push(record.evaluated as f64);
        self.tokens.push(record.completion.tokens as f64);
    }

    pub fn after_backspace(&mut self, record: &Record) {
        self.after_backspace.push(record.evaluated as f64);
    }

    pub fn record(&mut self, record: &Record, shown: Option<&Shown>, saved: usize) {
        self.latency_only(record);
        self.requests += 1;
        self.saved += saved;
        if record.evaluated > MISS_TOKENS {
            self.misses.push((record.position, record.evaluated));
        }
        if at_word_start(&record.before) {
            self.word_total += 1;
            let wanted = next_word(&record.rest).trim();
            let hit =
                shown.is_some_and(|s| !wanted.is_empty() && next_word(&s.text).trim() == wanted);
            self.word_hits += usize::from(hit);
        }
        let Some(shown) = shown else { return };
        self.shown += 1;
        if shown.wrong() {
            self.wrong += 1;
        } else if shown.exact() {
            self.exact += 1;
        } else {
            self.partial += 1;
        }
    }

    pub fn add_chars(&mut self, chars: usize) {
        self.chars += chars;
    }

    pub fn merge(&mut self, other: &Stats) {
        self.ms.extend(&other.ms);
        self.evaluated.extend(&other.evaluated);
        self.tokens.extend(&other.tokens);
        self.after_backspace.extend(&other.after_backspace);
        self.word_hits += other.word_hits;
        self.word_total += other.word_total;
        self.saved += other.saved;
        self.chars += other.chars;
        self.requests += other.requests;
        self.shown += other.shown;
        self.exact += other.exact;
        self.partial += other.partial;
        self.wrong += other.wrong;
    }
}

pub fn print_header() {
    println!(
        "{:<14} {:>4} {:>11} {:>7} {:>6} {:>6} {:>7} {:>6} {:>6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5}",
        "sample",
        "req",
        "word hit",
        "saved%",
        "shown",
        "exact",
        "partial",
        "wrong",
        "wrong%",
        "p50",
        "p90",
        "tok",
        "evp50",
        "evp90",
        "evmax",
        "miss",
        "bksp"
    );
}

pub fn print_row(name: &str, stats: &Stats) {
    println!(
        "{:<14} {:>4} {:>4}/{:<4}{:>2.0}% {:>6.1} {:>6} {:>6} {:>7} {:>6} {:>6.1} {:>5.0} {:>5.0} {:>5.0} {:>5.0} {:>5.0} {:>5.0} {:>5} {:>5.0}",
        name,
        stats.requests,
        stats.word_hits,
        stats.word_total,
        ratio(stats.word_hits, stats.word_total),
        ratio(stats.saved, stats.chars),
        stats.shown,
        stats.exact,
        stats.partial,
        stats.wrong,
        ratio(stats.wrong, stats.shown),
        percentile(&stats.ms, 0.5),
        percentile(&stats.ms, 0.9),
        percentile(&stats.tokens, 0.5),
        percentile(&stats.evaluated, 0.5),
        percentile(&stats.evaluated, 0.9),
        percentile(&stats.evaluated, 1.0),
        stats.misses.len(),
        percentile(&stats.after_backspace, 0.5),
    );
}

const MEAN_THRESHOLDS: [f32; 10] = [
    f32::NEG_INFINITY,
    -3.0,
    -2.5,
    -2.0,
    -1.6,
    -1.2,
    -1.0,
    -0.8,
    -0.6,
    -0.4,
];
const WORD_LIMITS: [usize; 7] = [2, 3, 4, 6, 8, 12, 16];

fn sweep_row(label: String, records: &[Record], config: &Config) {
    let (mut shown, mut exact, mut wrong, mut saved, mut shown_chars) = (0, 0, 0, 0, 0);
    for record in records {
        let Some(s) = show(record, config) else {
            continue;
        };
        shown += 1;
        saved += s.accepted;
        shown_chars += s.text.chars().count();
        if s.wrong() {
            wrong += 1;
        } else if s.exact() {
            exact += 1;
        }
    }
    println!(
        "{label:<18} shown {shown:>4}  exact {exact:>4}  wrong {wrong:>4}  wrong% {:>5.1}  saved-chars/request {:>5.2}  saved/shown chars {:>5.1}%",
        ratio(wrong, shown),
        saved as f64 / records.len().max(1) as f64,
        ratio(saved, shown_chars),
    );
}

pub fn print_sweep(records: &[Record], config: &Config) {
    println!(
        "\nsweep over {} requests at fixed positions (post hoc, same completions)",
        records.len()
    );
    for threshold in MEAN_THRESHOLDS {
        let tuned = Config {
            min_mean_logprob: threshold,
            ..config.clone()
        };
        sweep_row(format!("min_mean {threshold}"), records, &tuned);
    }
    for words in WORD_LIMITS {
        let tuned = Config {
            max_words: words,
            ..config.clone()
        };
        sweep_row(format!("max_words {words}"), records, &tuned);
    }
}
