use std::path::PathBuf;
use std::time::Instant;

use pith_core::{Completer, Config, FieldSnapshot, build, clean_completion, split_partial};
use pith_infer::LlamaModelHandle;

const SAMPLES: [(&str, &str); 6] = [
    (
        "Re: Q3 vendor contract - Gmail",
        "Hi Sarah, thanks for sending the revised contract. I reviewed the payment terms and they look good to me. Could you send over the final version so we can sign it by Friday?",
    ),
    (
        "#design-team | Acme - Slack",
        "Hey team, quick update on the landing page. The new hero section is live on staging and I would love your feedback before we ship it tomorrow morning.",
    ),
    (
        "Weekly sync notes - Notion",
        "Action items from today: Priya will follow up with the legal team about the data processing agreement, and Tom will share the updated roadmap by Wednesday.",
    ),
    (
        "Untitled - Notepad",
        "I am writing to let you know that I will be out of the office next week. Please reach out to Daniel if you need anything urgent while I am away.",
    ),
    (
        "Respuesta - Gmail",
        "Hola Marta, muchas gracias por tu mensaje. Te confirmo que la reunión será el jueves a las diez de la mañana en la oficina principal.",
    ),
    (
        "संदेश - WhatsApp",
        "नमस्ते भैया, मैं कल सुबह दस बजे आपके घर आऊँगा। कृपया दस्तावेज़ तैयार रखना।",
    ),
];

fn percentile(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    values[((values.len() - 1) as f64 * p).round() as usize]
}

fn main() {
    let mut args = std::env::args().skip(1);
    let model = PathBuf::from(args.next().expect("model path"));
    let threads: usize = args.next().map_or(4, |t| t.parse().unwrap());
    let config = Config {
        min_mean_logprob: f32::NEG_INFINITY,
        ..Config::default()
    };

    let started = Instant::now();
    let handle = LlamaModelHandle::load(&model).expect("load");
    let mut session = handle
        .session(threads, config.context_tokens)
        .expect("session");
    println!(
        "model {} threads {threads} load {:.0} ms",
        model.file_name().unwrap().to_string_lossy(),
        started.elapsed().as_secs_f64() * 1e3
    );

    let never = || false;
    let mut warm = Vec::new();
    let mut evaluated = Vec::new();
    let mut word_hits = 0usize;
    let mut word_total = 0usize;
    let mut chars_saved = 0usize;
    let mut chars_total = 0usize;

    for (title, text) in SAMPLES {
        let chars: Vec<char> = text.chars().collect();
        chars_total += chars.len();
        let field = |n: usize| FieldSnapshot {
            app: "bench".into(),
            window_title: title.into(),
            before: chars[..n].iter().collect(),
            ..Default::default()
        };
        let cold_started = Instant::now();
        let cold = session
            .complete(&build(&field(12), &config), &never)
            .expect("cold");
        println!(
            "\n[{title}] cold {:.0} ms ({} prompt tokens)",
            cold_started.elapsed().as_secs_f64() * 1e3,
            cold.prompt_tokens_evaluated
        );
        let mut position = 12;
        let mut step = 0;
        while position < chars.len() {
            let snapshot = field(position);
            let request = build(&snapshot, &config);
            let t = Instant::now();
            let completion = session.complete(&request, &never).expect("complete");
            let ms = t.elapsed().as_secs_f64() * 1e3;
            warm.push(ms);
            evaluated.push(completion.prompt_tokens_evaluated as f64);
            let suggestion = clean_completion(
                &completion.text,
                config.max_words,
                completion.stop == pith_core::StopReason::MaxTokens,
            )
            .unwrap_or_default();
            let suggestion = if snapshot.before.ends_with(' ') {
                suggestion.trim_start().to_string()
            } else {
                suggestion
            };
            let rest: String = chars[position..].iter().collect();
            let (_, partial) = split_partial(&snapshot.before);
            let at_word_start = snapshot.before.ends_with(' ') && partial == " ";
            if at_word_start {
                word_total += 1;
                let next_word = rest.split_whitespace().next().unwrap_or_default();
                let first = suggestion.split_whitespace().next().unwrap_or_default();
                if !next_word.is_empty() && first == next_word {
                    word_hits += 1;
                }
            }
            let common = suggestion
                .chars()
                .zip(rest.chars())
                .take_while(|(a, b)| a == b)
                .count();
            let accepted = if common == suggestion.chars().count() {
                common
            } else {
                0
            };
            if step % 6 == 0 {
                println!(
                    "  {ms:>6.0} ms  eval {:>3}  …{:<28} ⟶ {suggestion:?}",
                    completion.prompt_tokens_evaluated,
                    snapshot
                        .before
                        .chars()
                        .rev()
                        .take(28)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<String>()
                );
            }
            if accepted > 0 {
                chars_saved += accepted;
                position += accepted;
            } else {
                position += 1 + step % 3;
            }
            step += 1;
        }
        let back = field(chars.len().saturating_sub(6));
        let t = Instant::now();
        let after_backspace = session
            .complete(&build(&back, &config), &never)
            .expect("backspace");
        println!(
            "  backspace x6: {:.0} ms, eval {}",
            t.elapsed().as_secs_f64() * 1e3,
            after_backspace.prompt_tokens_evaluated
        );
    }

    let median = percentile(&mut warm.clone(), 0.5);
    let p90 = percentile(&mut warm.clone(), 0.9);
    let eval_median = percentile(&mut evaluated, 0.5);
    println!(
        "\nwarm per-keystroke latency: median {median:.0} ms, p90 {p90:.0} ms over {} requests",
        warm.len()
    );
    println!("prompt tokens evaluated per request: median {eval_median:.0}");
    println!(
        "next-word exact match at word starts: {word_hits}/{word_total} = {:.0}%",
        100.0 * word_hits as f64 / word_total.max(1) as f64
    );
    println!(
        "chars saved by accepting exact-prefix suggestions: {chars_saved}/{chars_total} = {:.0}%",
        100.0 * chars_saved as f64 / chars_total as f64
    );
}
