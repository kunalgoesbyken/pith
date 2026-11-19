mod cues;
mod personas;
mod report;
mod samples;

use std::path::PathBuf;
use std::time::Instant;

use cues::Mode;
use personas::Style;
use pith_core::{
    After, Completer, Completion, Config, FieldParts, FieldSnapshot, Primer, RawCue, StyleMemory,
    Surrounding, build, estimate_tokens,
};
use pith_infer::{LlamaModelHandle, LlamaSession};
use report::{
    Prompt, Record, Shown, Stats, print_estimator, print_header, print_row, print_sweep, show,
};
use samples::Sample;

const JUNK: [&str; 2] = ["zq", " mistaken"];
const CORRECTION_EVERY: usize = 4;
const SAMPLE_FIELD: u64 = 0;
const DISTRACTION_FIELD: u64 = 1;
const DISTRACTION_SPAN: usize = 40;
const DISTRACTION_TITLE: &str = "Notes";
const LEAVE_ONE_OUT_EVERY: usize = 4;
const DISTRACTION_TEXT: &str = "Remember to book the venue for the offsite and send the agenda to the whole team before Friday. Also ask finance whether the budget for travel has been approved yet so we can";

struct Options {
    model: PathBuf,
    threads: usize,
    config: Config,
    only: Option<String>,
    stride: Option<usize>,
    distract_every: usize,
    nearby: Mode,
    style: Style,
    eval: Style,
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
        nearby: Mode::Off,
        style: Style::Off,
        eval: Style::Off,
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
            "nearby" => options.nearby = parse(key, value),
            "nearby_tokens" => config.nearby_tokens = parse(key, value),
            "style" => options.style = parse(key, value),
            "eval" => options.eval = parse(key, value),
            "sweep" => options.sweep = parse(key, value),
            "verbose" => options.verbose = parse(key, value),
            _ => panic!("unknown option {key}"),
        }
    }
    options.config.nearby_context = options.nearby != Mode::Off;
    options.config.style_memory = options.style != Style::Off;
    options
}

struct Query<'a> {
    field_id: u64,
    title: &'a str,
    before: String,
    after: &'a str,
    cues: &'a [RawCue],
}

fn ask(
    session: &mut LlamaSession,
    config: &Config,
    memory: &StyleMemory,
    query: Query,
) -> (Completion, f64, Prompt, Primer) {
    let field = FieldSnapshot::from_parts(
        config,
        FieldParts {
            field_id: query.field_id,
            app: Some("bench".into()),
            window_title: query.title.into(),
            surrounding: Some(Surrounding {
                before: query.before,
                after: query.after.into(),
            }),
            has_selection: false,
            is_password: false,
            composing: false,
            language: None,
            placement: None,
            style: Default::default(),
            nearby: query.cues.to_vec(),
        },
    );
    let primer = Primer::select(memory, &field, config);
    let request = build(&field, config, &primer);
    let prompt = Prompt {
        tokens: session.count_tokens(&request.prompt),
        estimate: estimate_tokens(&request.prompt),
    };
    let started = Instant::now();
    let completion = session.complete(&request, &|| false).expect("complete");
    (
        completion,
        started.elapsed().as_secs_f64() * 1e3,
        prompt,
        primer,
    )
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
    (sample, memory): (&Sample, &StyleMemory),
    all: &mut Vec<Record>,
) -> Stats {
    let chars: Vec<char> = sample.text.chars().collect();
    let config = &options.config;
    let cues = cues::raw(&sample.name, options.nearby);
    let mut stats = Stats::default();
    let mut position = sample.start.min(chars.len().saturating_sub(1));
    stats.add_chars(chars.len() - position);
    let (mut step, mut after_junk, mut cold) = (0, false, true);
    while position < chars.len() {
        let prefix: String = chars[..position].iter().collect();
        if sample.corrections && step % CORRECTION_EVERY == 2 {
            let junk = format!("{prefix}{}", JUNK[step / CORRECTION_EVERY % JUNK.len()]);
            let (completion, ms, prompt, primer) = ask(
                session,
                config,
                memory,
                Query {
                    field_id: SAMPLE_FIELD,
                    title: sample.title,
                    before: junk.clone(),
                    after: sample.suffix,
                    cues: &cues,
                },
            );
            stats.latency_only(&Record {
                position,
                before: junk,
                after: sample.suffix.into(),
                rest: String::new(),
                evaluated: completion.prompt_tokens_evaluated,
                completion,
                ms,
                prompt,
                primer,
            });
            after_junk = true;
        }
        if options.distract_every > 0 && step % options.distract_every == options.distract_every - 1
        {
            let typed = DISTRACTION_TEXT.len() - DISTRACTION_SPAN
                + (step / options.distract_every) % DISTRACTION_SPAN;
            let text = &DISTRACTION_TEXT[..typed];
            let (completion, ms, prompt, primer) = ask(
                session,
                config,
                memory,
                Query {
                    field_id: DISTRACTION_FIELD,
                    title: DISTRACTION_TITLE,
                    before: text.to_string(),
                    after: "",
                    cues: &[],
                },
            );
            stats.latency_only(&Record {
                position,
                before: String::new(),
                after: After::default(),
                rest: String::new(),
                evaluated: completion.prompt_tokens_evaluated,
                completion,
                ms,
                prompt,
                primer,
            });
        }
        let (completion, ms, prompt, primer) = ask(
            session,
            config,
            memory,
            Query {
                field_id: SAMPLE_FIELD,
                title: sample.title,
                before: prefix.clone(),
                after: sample.suffix,
                cues: &cues,
            },
        );
        let record = Record {
            position,
            before: prefix,
            after: sample.suffix.into(),
            rest: chars[position..].iter().collect(),
            evaluated: completion.prompt_tokens_evaluated,
            completion,
            ms,
            prompt,
            primer,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Standard,
    HeldOut,
    LeaveOneOut,
}

struct Case {
    sample: Sample,
    memory: StyleMemory,
    group: Group,
}

fn cases(options: &Options) -> Vec<Case> {
    let memory = options.style.memory();
    let mut cases: Vec<Case> = samples::all()
        .into_iter()
        .map(|sample| Case {
            sample,
            memory: memory.clone(),
            group: Group::Standard,
        })
        .collect();
    let Some(persona) = options.eval.persona() else {
        return cases;
    };
    for (index, text) in persona.held_out.iter().enumerate() {
        cases.push(Case {
            sample: samples::notes(format!("held-{index}"), text),
            memory: memory.clone(),
            group: Group::HeldOut,
        });
    }
    let every = persona.memory.entries().iter().enumerate();
    for (index, entry) in every.step_by(LEAVE_ONE_OUT_EVERY) {
        let mut without = memory.clone();
        without.forget(&entry.app, entry.sentence.as_str());
        cases.push(Case {
            sample: samples::notes(format!("loo-{index}"), entry.sentence.as_str()),
            memory: without,
            group: Group::LeaveOneOut,
        });
    }
    cases
}

fn main() {
    let options = options();
    let started = Instant::now();
    let handle = LlamaModelHandle::load(&options.model).expect("load");
    let mut session = handle
        .session(options.threads, options.config.context_tokens)
        .expect("session");
    println!(
        "model {} threads {} load {:.0} ms\nconfig: min_mean {} min_token {} max_words {} max_tokens {} context_chars {} context_tokens {} nearby {:?} nearby_tokens {} style {:?} eval {:?}\n",
        options.model.file_name().unwrap().to_string_lossy(),
        options.threads,
        started.elapsed().as_secs_f64() * 1e3,
        options.config.min_mean_logprob,
        options.config.min_token_logprob,
        options.config.max_words,
        options.config.max_tokens,
        options.config.context_chars,
        options.config.context_tokens,
        options.nearby,
        options.config.nearby_tokens,
        options.style,
        options.eval,
    );
    let mut total = Stats::default();
    let mut midline = Stats::default();
    let mut held = Stats::default();
    let mut left_out = Stats::default();
    let mut overall = Stats::default();
    let mut rows = Vec::new();
    let mut records = Vec::new();
    for case in cases(&options) {
        let sample = &case.sample;
        if options
            .only
            .as_ref()
            .is_some_and(|only| !sample.name.contains(only.as_str()))
        {
            continue;
        }
        let stats = replay(&mut session, &options, (sample, &case.memory), &mut records);
        let group = match (case.group, sample.suffix.is_empty()) {
            (Group::Standard, true) => &mut total,
            (Group::Standard, false) => &mut midline,
            (Group::HeldOut, _) => &mut held,
            (Group::LeaveOneOut, _) => &mut left_out,
        };
        group.merge(&stats);
        overall.merge(&stats);
        rows.push((sample.name.clone(), stats));
    }
    println!();
    print_header();
    for (name, stats) in &rows {
        print_row(name, stats);
    }
    print_row("TOTAL", &total);
    print_row("MID-LINE", &midline);
    print_row("HELD-OUT", &held);
    print_row("LEAVE-ONE-OUT", &left_out);
    print_estimator("ALL", &overall);
    for (name, stats) in &rows {
        print_estimator(name, stats);
    }
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
