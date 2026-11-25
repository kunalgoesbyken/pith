# How it works

## Windows

- **Reads text** through Windows UI Automation on a background thread with 250 ms timeouts, so a hung app never freezes Pith.
- **Notices typing** through WinEvents (focus, value, caret), not a keyboard hook.
- **Captures the accept keys and Esc** with `RegisterHotKey` only while a suggestion is visible, and releases them right after. Right at a caret that is not at the end of the text goes to the app untouched.
- **Draws** the suggestion in a click-through, non-activating layered window, in the field's own font and text colour at reduced opacity.
- **Predicts** with llama.cpp in-process. The KV cache is reused across keystrokes. A half-typed word is removed from the prompt and forced as the start of the generation, so the suggestion finishes the word. Low-confidence suggestions are dropped.
- **Checks spelling** of the word you just finished through the Windows Spell Checking API on its own thread, in the language of your keyboard layout. A correction takes the place of the completion for that word; the completion waits for the answer and is never delayed past its normal pause. Applying it selects the word with UI Automation, re-reads it and pastes the fix.
- **Skips** password fields, IME composition, selections, text after the caret on the same line (unless it is only closing punctuation, which shows in the pill), terminals, password managers, sign-in screens and, until you allow them, code editors.
- **Re-reads the field** when you press an accept key or Esc, and acts only if the suggestion still fits what is on screen.

```
crates/core      engine, config, policy, placement, stats (platform-free, unit-tested)
crates/infer     llama.cpp session, worker thread, model download, remote backend
crates/windows   UI Automation, WinEvents, overlay, pill, hotkeys, tray, settings, onboarding
crates/linux     AT-SPI reader, X11 overlay, key grabs, insertion, event loop (X11 only)
crates/app       pith.exe on Windows, pith on Linux
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
- `e2e` drives the whole app against its own RichEdit windows through window messages, so it never sends keystrokes to other apps. It covers long documents, several scripts, accept chains, stale keys, Esc, typo fixes, selections, password fields, text and punctuation after the caret, pause, field switching, the fallback pill, overlay geometry and memory. `PITH_E2E_ONLY=<name>` runs matching scenarios, `PITH_IDLE_UNLOAD=<secs>` adds the unload and reload scenario, `PITH_E2E_INSERT_LAG_MS=<ms>` delays insertion like a real paste and `PITH_E2E_TRACE=1` prints every action.
- `apps` runs the whole app against real Chrome, Edge, Electron and Notepad on a GitHub Actions Windows runner (workflow `Apps`, weekly or by hand) and reports, per app and field, whether Pith reads the text and caret, shows a suggestion and inserts it on Tab. It sends real keystrokes and mouse clicks, so it refuses to start unless it runs on a GitHub-hosted runner with `--ci-runner`, and is never meant for a desktop in use. Firefox is not covered: it shows no page under the limited token the harness has to use, and Pith skips elevated apps.
- `ui_shots` renders the settings and welcome windows, light and dark, to PNGs in `target/ui-shots`.

## Roadmap

- Tested support for Chrome, Edge, Slack, Teams, Outlook, Word and WhatsApp
- Suggestions before words in the middle of a line, which needs a model that reads what follows the caret
- Snippets and per-site rules
- Several candidates to choose from
- Code signing, winget and automatic update notices
- macOS (Accessibility API) and Linux on Wayland (Linux on X11 is experimental)
