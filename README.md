<h1 align="center"><img src="assets/logo.svg" alt="Pith" width="280"></h1>

Local autocomplete for every text box on Windows. Pause while you type and Pith shows the rest of your sentence in grey at the caret. Tab keeps all of it, Shift+Tab or Ctrl+Right keeps one word, Esc hides it, and Ctrl+Shift+Space asks for a suggestion right away.

A 0.6B model runs on your CPU. There is no account and no telemetry. The only network use is the one-time model download and the update check you start yourself. It is built for ordinary laptops with 8 GB RAM and no GPU.

Status: Windows MVP. macOS and Linux are planned; the engine and inference crates are already platform-free.

## What stays on your PC

| | |
|---|---|
| Text you type | Read from the focused field only, held in memory for the prompt, never written to disk or sent anywhere |
| Network | Two things, and only these. One download of the model from Hugging Face (pinned revision, SHA-256 checked). One request to `api.github.com` for the releases of this repository, only when you click Check for updates in the tray; it sends no text and no identifier, and nothing is downloaded or installed. Nothing else, unless you set `remote_url` |
| Telemetry | None |
| Keystrokes | Never seen. There is no keyboard hook |
| Password fields, password managers, sign-in screens, blocked apps | Never read |
| Clipboard | Accepting pastes through the clipboard, excluded from clipboard history, cloud clipboard and clipboard monitors, then restores what you had |
| On disk | `config.toml` (your settings), `stats.toml` (counts of suggestions shown and accepted, no text), `pith.log` (timings, errors and app names, capped at 1 MB) and the model |

Settings > Delete model and data removes everything except your settings.

With `remote_url` set, the text before the caret is sent to that server. Pith accepts only `https`, or `http` to this computer. The tray tooltip, the Settings footer and the welcome window then name the server, and say "on this PC" only when it really is.

## Numbers

i5-9300H laptop, CPU only, 4 threads, `standard` model. Latency and accuracy come from `typing_bench` replaying 12 samples in 9 languages (English email, chat and Slack, German, Spanish, Hindi, Japanese, Chinese, lists, numbers and URLs, edits, a long document). Memory and the first-suggestion time come from `e2e`.

| | |
|---|---|
| Model work per keystroke while typing | median 229 ms, p90 577 ms, plus the 120 ms pause detection |
| First suggestion in a field you just opened | 1.1-1.6 s (the context is read once, then reused) |
| Keystrokes saved if every right suggestion is accepted | 29% |
| Next word shown and right, at the start of a word | 12% |
| RAM with the model loaded | ~610 MB |
| RAM idle | ~30 MB before the first suggestion, ~50 MB after the model unloads |
| Binary | one 7.4 MB exe, no webview, no installer, no runtime DLLs |

Per sample (`standard`, offline replay; "saved" assumes every right suggestion is accepted, "wrong" is the share of shown suggestions that disagree with what was typed next):

| Sample | Saved | Next word right | Wrong | p50 | p90 |
|---|---|---|---|---|---|
| English work email | 23% | 16% | 22% | 228 ms | 338 ms |
| English casual chat | 6% | 0% | 54% | 205 ms | 362 ms |
| English Slack | 21% | 10% | 34% | 202 ms | 310 ms |
| English bullet lists | 20% | 13% | 37% | 162 ms | 284 ms |
| English with numbers and URLs | 23% | 7% | 46% | 323 ms | 640 ms |
| English with corrections | 56% | 36% | 18% | 183 ms | 283 ms |
| English long document | 34% | 15% | 28% | 232 ms | 429 ms |
| German | 44% | 20% | 33% | 373 ms | 688 ms |
| Spanish | 40% | 18% | 29% | 559 ms | 1317 ms |
| Hindi | 21% | 5% | 61% | 749 ms | 1927 ms |
| Japanese | 44% | 0% | 54% | 144 ms | 209 ms |
| Chinese | 16% | 0% | 50% | 133 ms | 219 ms |

Hindi and Spanish are the slowest because they take more tokens per word. Japanese and Chinese have no spaces, so "next word right" rarely applies; "saved" is the better measure there. Casual chat is the hardest: short, unpredictable replies.

## Use it

Download `pith.exe` from [Releases](https://github.com/kunalgoesbyken/pith/releases) and run it; `pith.exe.sha256` next to it lets you check the file. The exe is not code-signed yet, so Windows SmartScreen may ask you to confirm the first start. On first start Pith downloads the model into `%LOCALAPPDATA%\Pith\data\models`, then shows a tray icon.

| `model` | File | Download | RAM loaded | Median / p90 per keystroke | Keystrokes saved | Exact next word | Notes |
|---|---|---|---|---|---|---|---|
| `small` | SmolLM2-360M Q4_K_M | 271 MB | ~370 MB | 148 / 306 ms | 26% | 11% | English-focused, loads in 0.65 s |
| `standard` | Qwen3-0.6B-Base Q4_K_M | 397 MB | ~590 MB | 206 / 420 ms | 29% | 12% | multilingual, the common choice |
| `large` | Qwen3-1.7B-Base Q4_K_M | 1107 MB | ~1270 MB | 520 / 1316 ms | 37% | 14% | multilingual, best suggestions, about 2.5 times slower than standard |

Measured with `typing_bench` on one PC (4 threads, 8 logical cores, 16 GB; 1,200 to 1,400 keystrokes over 12 samples). RAM is the peak private memory of the process while loaded. Latency depends on your CPU, so read the milliseconds as relative. The Numbers section above is from a slower laptop.

Pith recommends a model from the memory and logical cores it finds: `large` with at least 13.2 GB of RAM and 12 cores, `standard` with at least 5.6 GB and 4 cores, `small` otherwise. Until you choose one in Settings, Pith uses the recommendation.

Pith needs a CPU with AVX2 (most PCs since 2015). On older CPUs the tray says so instead of crashing, and `remote_url` lets you use a server instead.

The tray shows what Pith is doing (Ready, Paused until 14:35, Skipped: password field, Loading model). It pauses Pith for 15 minutes, an hour or until you resume, and turns it off in the app you were just using. Check for updates in the same menu asks GitHub once for the latest release and shows the result there; the release page opens in your browser only when you click it. In a code editor, which Pith skips by default, the same item reads Allow Pith in that app.

Settings changes apply at once:

- Enabled, and how often suggestions appear (Fewer, Normal, More)
- Suggestion length (Short, Normal, Long)
- The model (Small, Standard or Large, with the recommended one marked). Choosing one downloads it if needed and switches without a restart; downloaded models are kept
- Ghost text opacity
- Your stats: suggestions seen and accepted today and in total, and the time they saved you
- The apps with their own setting: where Pith is turned off, and the code editors where you turned it on
- Start with Windows (off by default)
- Open data folder, Delete model and data

Settings also warns when something else suggests in the same fields: Windows text suggestions for hardware keyboards (with a button to turn them off), PhraseExpress or Lightkey.

When an app reports no caret, or a caret Pith cannot trust, the suggestion shows in a small pill just below the field, or inside its bottom edge when there is no room. The pill keeps its own background and contrast, so the opacity setting does not apply to it.

The pill is also used when the only text after the caret on the line is closing punctuation, such as `)`, `]`, `"`, `.`, `?`, `」` or `）。`. Pith suggests the words that belong before it, shows them in a pill just below the caret so they never cover the text that is already there, and drops any part of the suggestion that repeats the punctuation (`could you send it` before `?`, not `could you send it?`). Accepting inserts at the caret and leaves the punctuation where it is. Any other text after the caret on the line (a word, an opening bracket, a symbol) still skips the suggestion.

Pith pauses and frees the model while Windows battery saver is on.

## Config

Every setting lives in `%APPDATA%\Pith\config\config.toml`. An invalid value is reported in the tray, and the defaults are used until you fix it.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | true | suggest at all |
| `model` | recommended for your PC | `small`, `standard` or `large`; absent until you choose one in Settings |
| `model_path` | none | any GGUF file; overrides `model` |
| `threads` | 0 | 0 = half the logical cores, at most 4 |
| `context_tokens` | 1024 | model context; at most 16384 and at least `max_tokens` + 64 |
| `context_chars` | 1200 | characters before the caret used as the prompt |
| `min_chars_before` | 3 | characters needed before suggesting automatically |
| `debounce_ms` | 120 | pause before asking the model |
| `max_words` | 6 | longest suggestion; set by Suggestion length |
| `max_tokens` | 16 | token budget for one suggestion; set by Suggestion length |
| `min_mean_logprob` | -1.6 | confidence gate; raise it for fewer, safer suggestions. A remote server that returns no logprobs has unknown confidence, so this gate lets its suggestions through |
| `min_token_logprob` | -3.5 | a suggestion stops at the first token below this |
| `ghost_opacity` | 50 | ghost text strength, 10 to 100 percent; raised automatically to keep 3:1 contrast |
| `trigger_mode` | auto | `auto` suggests as you pause; `manual` only on the force-trigger key |
| `accept_keys` | ["Tab"] | keys that accept the whole suggestion, e.g. `["Tab", "Right"]` (Right accepts only at the end of the text) |
| `accept_word_keys` | ["Shift+Tab", "Ctrl+Right"] | keys that accept one word |
| `accept_race_ms` | 150 | a key pressed this soon after a suggestion appears goes to the app, so a Tab you were already typing is not swallowed |
| `force_trigger` | true | register the force-trigger key |
| `force_trigger_key` | Ctrl+Shift+Space | asks for a suggestion now, even where short or non-prose text is skipped |
| `idle_unload_secs` | 300 | free the model after this much idle time (0 = never) |
| `pause_on_battery_saver` | true | pause and free the model while battery saver is on; PCs without a battery are unaffected |
| `auto_pause_after` | 12 | stop suggesting in an app after this many ignored suggestions in a row (0 = never); the tray offers Resume |
| `smart_space` | false | add a space after accepting the last word of a suggestion |
| `blocked_apps` | [] | extra process names to ignore. Terminals, password managers and sign-in screens are always ignored, and so are code editors unless listed in `allowed_apps` |
| `allowed_apps` | [] | code editors to turn Pith on in: `Code`, `Cursor`, `devenv`, `idea64`, `pycharm64`, `rider64`, `zed`. Any other name, including a terminal or password manager, is ignored. Pith completes prose, not code: it skips only lines that are mostly symbols or numbers, so expect suggestions in comments and some in code |
| `typing_wpm` | 40 | your typing speed, used for the time-saved estimate (1 to 500) |
| `remote_url` | none | use an OpenAI-compatible `/v1/completions` server instead of the local model; `https`, or `http` to this computer |
| `remote_model` | none | the `model` name sent to that server |

## How it works

- **Reads text** through Windows UI Automation on a background thread with 250 ms timeouts, so a hung app never freezes Pith.
- **Notices typing** through WinEvents (focus, value, caret), not a keyboard hook.
- **Captures the accept keys and Esc** with `RegisterHotKey` only while a suggestion is visible, and releases them right after. Right at a caret that is not at the end of the text goes to the app untouched.
- **Draws** the suggestion in a click-through, non-activating layered window, in the field's own font and text colour at reduced opacity.
- **Predicts** with llama.cpp in-process. The KV cache is reused across keystrokes. A half-typed word is removed from the prompt and forced as the start of the generation, so the suggestion finishes the word. Low-confidence suggestions are dropped.
- **Skips** password fields, IME composition, selections, text after the caret on the same line (unless it is only closing punctuation, found by Unicode category), terminals, password managers, sign-in screens and, until you allow them, code editors.
- **Re-reads the field** when you press an accept key or Esc, and acts only if the suggestion still fits what is on screen.

```
crates/core      engine, config, policy, placement, stats (platform-free, unit-tested)
crates/infer     llama.cpp session, worker thread, model download, remote backend
crates/windows   UI Automation, WinEvents, overlay, pill, hotkeys, tray, settings, onboarding
crates/app       pith.exe
```

## Build and test

Needs Rust (MSVC toolchain), CMake and libclang. Without system installs: `pip install --user cmake libclang`, put the CMake `bin` on `PATH`, and point `LIBCLANG_PATH` at the `clang/native` folder of the libclang package. `.cargo/config.toml` links the C runtime statically, so the exe runs on a clean Windows install.

```
cargo build --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run --release -p pith-infer --example typing_bench -- path\to\model.gguf 4
cargo run --release -p pith-windows --example e2e -- path\to\model.gguf .
cargo run --release -p pith-windows --example ui_shots
```

- `typing_bench` replays realistic typing keystroke by keystroke and reports latency, cache reuse and how often the suggestion matches what was typed next. The MID-LINE row replays text typed before closing punctuation and is kept out of TOTAL.
- `e2e` drives the whole app against its own RichEdit windows through window messages, so it never sends keystrokes to other apps. It covers long documents, several scripts, accept chains, stale keys, Esc, selections, password fields, text and punctuation after the caret, pause, field switching, the fallback pill, overlay geometry and memory. `PITH_E2E_ONLY=<name>` runs matching scenarios, `PITH_IDLE_UNLOAD=<secs>` adds the unload and reload scenario, `PITH_E2E_INSERT_LAG_MS=<ms>` delays insertion like a real paste and `PITH_E2E_TRACE=1` prints every action.
- `ui_shots` renders the settings and welcome windows, light and dark, to PNGs in `target/ui-shots`.

## Roadmap

- Tested support for Chrome, Edge, Slack, Teams, Outlook, Word and WhatsApp
- Learns your phrasing from text you accept, kept on this PC
- Context from the rest of the window, such as the email you are replying to
- Suggestions before words in the middle of a line, which needs a model that reads what follows the caret
- Typo fixes alongside the completion
- Snippets and per-site rules
- Several candidates to choose from
- Code signing, winget and automatic update notices
- macOS (Accessibility API) and Linux (AT-SPI)

## License

MIT
