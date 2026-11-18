# Pith

Local autocomplete for every text box. Pause while typing in any app and Pith shows the rest of your sentence in grey at the caret. Tab keeps it, Shift+Tab keeps one word, Esc hides it.

Everything runs on your machine: a 0.6B model on the CPU, no account, no network after the one-time model download. It is built for ordinary laptops: 8 GB RAM, no GPU.

Status: Windows MVP. macOS and Linux are planned (the engine and inference crates are platform-free).

## Numbers (i5-9300H laptop, CPU only, 4 threads)

| | |
|---|---|
| Last keystroke to ghost text | 230-390 ms (includes the 120 ms pause detection) |
| Model work per keystroke | median 160 ms, 1 prompt token re-evaluated |
| RAM with the model loaded | ~520 MB |
| RAM when idle (model unloads after 5 min) | ~30 MB |
| Binary | one 6.9 MB exe, no webview, no installer, no runtime DLLs to install |
| Next word predicted exactly | ~30% of word starts (offline replay) |

## How it works

- **Reads text** through Windows UI Automation on a background thread with 250 ms timeouts, so a hung app never freezes Pith.
- **Notices typing** through WinEvents (focus, value, caret). There is no keyboard hook, so Pith never sees your keystrokes.
- **Captures Tab / Shift+Tab / Esc** with `RegisterHotKey` only while a suggestion is visible, and releases them right after.
- **Draws** the suggestion in a click-through, non-activating layered window, in the field's own font and text colour at reduced opacity.
- **Inserts** accepted text with a clipboard paste that is hidden from clipboard history and restores your clipboard afterwards.
- **Predicts** with llama.cpp embedded in-process. The KV cache is reused across keystrokes. A half-typed word is removed from the prompt and forced as the start of the generation, so suggestions finish the word correctly. Low-confidence suggestions are dropped.
- **Skips** password fields, IME composition, selections, text after the caret, terminals, code editors and password managers. Text in password fields and blocked apps is never read.
- **Re-reads the field** when you press Tab, Shift+Tab or Esc, and only acts if the suggestion still fits what is on screen.

```
crates/core      engine state machine, policy, prompt, text utils (pure, unit-tested)
crates/infer     llama.cpp session, worker thread, optional OpenAI-compatible remote backend
crates/windows   UI Automation reader, WinEvents, overlay, hotkeys, clipboard, tray, main loop
crates/app       pith.exe: config, model download, wiring
```

## Run

Download `pith.exe` and run it. On first start it downloads the model into `%LOCALAPPDATA%\Pith\data\models` (pinned revision, SHA-256 checked), then shows a tray icon.

| `model` | File | RAM loaded | Median per keystroke | Notes |
|---|---|---|---|---|
| `standard` (default) | Qwen3-0.6B-Base Q4_K_M, 397 MB | ~520 MB | ~165 ms | multilingual, best suggestions |
| `lite` | SmolLM2-360M Q4_K_M, 271 MB | ~330 MB | ~140 ms | English-focused, loads in 0.25 s |

Pith needs a CPU with AVX2 (most PCs since 2015). On older CPUs it says so in the tray instead of crashing; point `remote_url` at a server to use it there.

Invalid settings are reported in the tray and the defaults are used until you fix them. Settings live in `%APPDATA%\Pith\config\config.toml` (tray menu, "Open settings"):

| Key | Default | Meaning |
|---|---|---|
| `model` | standard | `standard` or `lite` |
| `model_path` | none | any GGUF file; overrides `model` |
| `threads` | 0 | 0 = half the logical cores, max 4 |
| `debounce_ms` | 120 | pause before asking the model |
| `max_words` | 6 | longest suggestion |
| `min_mean_logprob` | -1.6 | confidence gate; raise for fewer, safer suggestions |
| `ghost_opacity` | 50 | ghost text strength, 10 to 100 percent; raised automatically to keep 3:1 contrast |
| `idle_unload_secs` | 300 | free the model after this much idle time (0 = never) |
| `blocked_apps` | [] | extra process names to ignore; terminals, editors, password managers and sign-in screens are always ignored |
| `remote_url` | none | use an OpenAI-compatible `/v1/completions` server instead of the local model; must be `https`, or `http` to this computer |

## Build

Needs Rust (MSVC toolchain), CMake and libclang. Without system installs: `pip install --user cmake libclang`, then put the CMake `bin` on `PATH` and point `LIBCLANG_PATH` at the `clang/native` folder of the libclang package. `.cargo/config.toml` links the C runtime statically, so the exe runs on a clean Windows install.

```
cargo build --release
cargo test -p pith-core -p pith-infer
cargo run --release -p pith-infer --example typing_bench -- path\to\model.gguf 4
cargo run --release -p pith-windows --example e2e -- path\to\model.gguf .
```

`typing_bench` replays realistic typing keystroke by keystroke and reports latency, cache reuse and how often the suggestion matches what was typed. `e2e` drives the full app against its own RichEdit windows through window messages, so it never sends keystrokes to other apps. It covers long documents, several scripts, rapid accept chains, stale keys, escape, selection, password fields, pause, field switching, overlay geometry and memory. `PITH_E2E_ONLY=<name>` runs matching scenarios, `PITH_IDLE_UNLOAD=<secs>` adds the unload and reload scenario, `PITH_E2E_INSERT_LAG_MS=<ms>` delays insertion like the real paste, `PITH_E2E_TRACE=1` prints every action.

## Roadmap

- Test matrix for Chrome, Edge, Slack, Teams, Outlook, Word and WhatsApp
- Streaming the first word as soon as it is ready
- Per-field KV cache slots so switching apps does not re-read context
- Local session memory and an "ask" hotkey over your recent work, then fact-mismatch hints
- macOS (Accessibility API) and Linux (AT-SPI / input method)

## License

MIT
