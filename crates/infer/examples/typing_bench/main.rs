mod report;
mod samples;

use std::path::PathBuf;
use std::time::Instant;

use pith_core::{Completer, Completion, Config, FieldSnapshot, build};
use pith_infer::{LlamaModelHandle, LlamaSession};
use report::{Record, Shown, Stats, print_header, print_row, print_sweep, show};
use samples::Sample;

const JUNK: [&str; 2] = ["zq", " mistaken"];
const CORRECTION_EVERY: usize = 4;
const SAMPLE_FIELD: u64 = 0;
const DISTRACTION_FIELD: u64 = 1;
const DISTRACTION_SPAN: usize = 40;
const DISTRACTION_TITLE: &str = "Notes";
const DISTRACTION_TEXT: &str = "Remember to book the venue for the offsite and send the agenda to the whole team before Friday. Also ask finance whether the budget for travel has been approved yet so we can";

struct Options {
    model: PathBuf,
    threads: usize,
    config: Config,
    only: Option<String>,
    stride: Option<usize>,
    distract_every: usize,
    sweep: bool,
    verbose: bool,
}

fn parse<T: std::str::FromStr>(key: &str, value: &str) -> T {
    value
        .parse()
        .unwrap_or_else(|_| panic!("bad value for {key}: {value}"))
}

fn options() -> Options {
    let mut args = std::env::args().skip(1);
    let mut options = Options {
        model: PathBuf::from(args.next().expect("model path")),
        threads: 4,
        config: Config::default(),
        only: None,
        stride: None,
        distract_every: 0,
        sweep: false,
        verbose: false,
    };
    for arg in args {
        let Some((key, value)) = arg.split_once('=') else {
            options.threads = parse("threads", &arg);
            continue;
        };
        let config = &mut options.config;
        match key {
            "threads" => options.threads = parse(key, value),
            "min_mean" => config.min_mean_logprob = parse(key, value),
            "min_token" => config.min_token_logprob = parse(key, value),
            "max_words" => config.max_words = parse(key, value),
            "max_tokens" => config.max_tokens = parse(key, value),
            "context_chars" => config.context_chars = parse(key, value),
            "context_tokens" => config.context_tokens = parse(key, value),
            "only" => options.only = Some(value.to_string()),
            "stride" => options.stride = Some(parse(key, value)),
            "distract" => options.distract_every = parse(key, value),
            "sweep" => options.sweep = parse(key, value),
            "verbose" => options.verbose = parse(key, value),
            _ => panic!("unknown option {key}"),
        }
    }
    options
}

fn ask(
    session: &mut LlamaSession,
    config: &Config,
    field_id: u64,
    title: &str,
    (before, after): (String, &str),
) -> (Completion, f64) {
    let field = FieldSnapshot {
        field_id,
        app: "bench".into(),
        window_title: title.into(),
        before,
        after: after.into(),
        ..Default::default()
    };
    let started = Instant::now();
    let completion = session
        .complete(&build(&field, config), &|| false)
        .expect("complete");
    (completion, started.elapsed().as_secs_f64() * 1e3)
}

fn advance(shown: Option<&Shown>, step: usize, stride: Option<usize>) -> (usize, usize) {
    if let Some(stride) = stride {
        return (stride, 0);
    }
    match shown {
        Some(shown) if shown.accepted > 0 => (shown.accepted, shown.accepted),
        _ => (1 + step % 3, 0),
    }
}

fn replay(
    session: &mut LlamaSession,
    options: &Options,
    sample: &Sample,
    all: &mut Vec<Record>,
) -> Stats {
    let chars: Vec<char> = sample.text.chars().collect();
    let config = &options.config;
    let mut stats = Stats::default();
    let mut position = sample.start.min(chars.len().saturating_sub(1));
    stats.add_chars(chars.len() - position);
    let (mut step, mut after_junk, mut cold) = (0, false, true);
    while position < chars.len() {
        let prefix: String = chars[..position].iter().collect();
        if sample.corrections && step % CORRECTION_EVERY == 2 {
            let junk = format!("{prefix}{}", JUNK[step / CORRECTION_EVERY % JUNK.len()]);
            let (completion, ms) = ask(
                session,
                config,
                SAMPLE_FIELD,
                sample.title,
                (junk.clone(), sample.suffix),
            );
            stats.latency_only(&Record {
                position,
                before: junk,
                after: sample.suffix.to_string(),
                rest: String::new(),
                evaluated: completion.prompt_tokens_evaluated,
                completion,
                ms,
            });
            after_junk = true;
        }
        if options.distract_every > 0 && step % options.distract_every == options.distract_every - 1
        {
            let typed = DISTRACTION_TEXT.len() - DISTRACTION_SPAN
                + (step / options.distract_every) % DISTRACTION_SPAN;
            let text = &DISTRACTION_TEXT[..typed];
            let (completion, ms) = ask(
                session,
                config,
                DISTRACTION_FIELD,
                DISTRACTION_TITLE,
                (text.to_string(), ""),
            );
            stats.latency_only(&Record {
                position,
                before: String::new(),
                after: String::new(),
                rest: String::new(),
                evaluated: completion.prompt_tokens_evaluated,
                completion,
                ms,
            });
        }
        let (completion, ms) = ask(
            session,
            config,
            SAMPLE_FIELD,
            sample.title,
            (prefix.clone(), sample.suffix),
        );
        let record = Record {
            position,
            before: prefix,
            after: sample.suffix.to_string(),
            rest: chars[position..].iter().collect(),
            evaluated: completion.prompt_tokens_evaluated,
            completion,
            ms,
        };
        let shown = show(&record, config);
        let (moved, saved) = advance(shown.as_ref(), step, options.stride);
        if cold {
            println!(
                "  {:<14} cold {:.0} ms, {} prompt tokens",
                sample.name, record.ms, record.evaluated
            );
            cold = false;
        } else {
            stats.record(&record, shown.as_ref(), saved, config);
            if after_junk {
                stats.after_backspace(&record);
            }
        }
        after_junk = false;
        if options.verbose {
            println!(
                "  {:>5.0} ms ev {:>3} tok {:>2} pos {:>4} {:?} -> {:?} (rest {:?})",
                record.ms,
                record.evaluated,
                record.completion.tokens,
                position,
                record
                    .before
                    .chars()
                    .rev()
                    .take(24)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<String>(),
                shown.as_ref().map(|s| s.text.as_str()),
                record.rest.chars().take(24).collect::<String>(),
            );
        }
        if options.sweep {
            all.push(record);
        }
        position += moved;
        step += 1;
    }
    stats
}

fn main() {
    let options = options();
    let started = Instant::now();
    let handle = LlamaModelHandle::load(&options.model).expect("load");
    let mut session = handle
        .session(options.threads, options.config.context_tokens)
        .expect("session");
    println!(
        "model {} threads {} load {:.0} ms\nconfig: min_mean {} min_token {} max_words {} max_tokens {} context_chars {} context_tokens {}\n",
        options.model.file_name().unwrap().to_string_lossy(),
        options.threads,
        started.elapsed().as_secs_f64() * 1e3,
        options.config.min_mean_logprob,
        options.config.min_token_logprob,
        options.config.max_words,
        options.config.max_tokens,
        options.config.context_chars,
        options.config.context_tokens,
    );
    let mut total = Stats::default();
    let mut midline = Stats::default();
    let mut rows = Vec::new();
    let mut records = Vec::new();
    for sample in samples::all() {
        if options
            .only
            .as_ref()
            .is_some_and(|only| !sample.name.contains(only.as_str()))
        {
            continue;
        }
        let stats = replay(&mut session, &options, &sample, &mut records);
        if sample.suffix.is_empty() {
            total.merge(&stats);
        } else {
            midline.merge(&stats);
        }
        rows.push((sample.name, stats));
    }
    println!();
    print_header();
    for (name, stats) in &rows {
        print_row(name, stats);
    }
    print_row("TOTAL", &total);
    print_row("MID-LINE", &midline);
    for (name, stats) in &rows {
        if !stats.misses.is_empty() {
            println!(
                "prompt re-evaluations above threshold [{name}] (position, tokens): {:?}",
                stats.misses
            );
        }
    }
    if options.sweep {
        print_sweep(&records, &options.config);
    }
}
