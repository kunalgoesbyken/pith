<h1 align="center"><img src="assets/logo.svg" alt="Pith" width="280"></h1>

Local autocomplete for every text box on Windows, and experimentally on Linux with X11. Pause while you type and Pith shows the rest of your sentence in grey at the caret. Tab keeps all of it, Shift+Tab or Ctrl+Right keeps one word, Esc hides it, and Ctrl+Shift+Space asks for a suggestion right away. When you finish a misspelled word, Pith offers the fix instead (`teh → the`) and Tab applies it.

A 0.6B model runs on your CPU. There is no account and no telemetry. The only network use is the one-time model download and the update check you start yourself. It is built for ordinary laptops with 8 GB RAM and no GPU.

Status: Windows MVP. Linux (X11) is experimental, see the Linux section. macOS is planned; the engine and inference crates are platform-free.

## What stays on your PC

| | |
|---|---|
| Text you type | Read from the focused field only, held in memory for the prompt, never written to disk or sent anywhere |
| Nearby text | The label of the box, the subject and recipient fields and the last message around it, read only to build the prompt. Held in memory, never written to disk or logged, and never sent when `remote_url` is set. Never read for password fields, password managers, blocked apps or Pith's own windows. `nearby_context = false` turns it off |
| Network | Two things, and only these. One download of the model from Hugging Face (pinned revision, SHA-256 checked). One request to `api.github.com` for the releases of this repository, only when you click Check for updates in the tray; it sends no text and no identifier, and nothing is downloaded or installed. Nothing else, unless you set `remote_url` |
| Telemetry | None |
| Keystrokes | Never seen. Windows has no keyboard hook. Linux grabs only the accept keys and Esc while a suggestion is shown, and the force-suggestion key while a field Pith may read is focused; every other key goes straight to the app |
| Password fields, password managers, sign-in screens, blocked apps | Never read |
| Spell checking | The one word you just finished is checked by the spell checker built into Windows (the Windows Spell Checking API, Windows 8 and later), which runs on this PC and uses your own Windows dictionaries. Pith adds no network request for it |
| Clipboard | Windows: accepting pastes through the clipboard, excluded from clipboard history, cloud clipboard and clipboard monitors, then restores what you had. Linux: text goes in through the accessibility interface when the app supports it. Only when the field has no editable text interface or refuses the insert is it pasted through the clipboard. Before pasting, Pith saves the previous clipboard only if every format on offer is text (plain text, UTF-8 text or HTML) or part of the selection protocol; an image, files or any other format means nothing is pasted and the clipboard is left untouched. It serves the saved text again about 300 ms after the last request, but only while Pith still owns the clipboard, so something you copy meanwhile is left alone. Just before pressing Ctrl+V it checks that the same field and window still have the focus, and if the app has not asked for the text within one second it counts the paste as failed, restores the clipboard and pastes nothing; an app slower than that may paste nothing or the previous clipboard text. If Pith exits while it owns the clipboard it hands the contents to a running clipboard manager. Linux has no way to hide a paste from clipboard managers |
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

Download `pith.exe` from [Releases](https://github.com/kunalgoesbyken/pith/releases) and run it, or take the portable zip, which also holds the license and this readme. The exe is not code-signed, so Windows SmartScreen may ask you to confirm the first start; [Verify your download](#verify-your-download) shows how to check it first. On first start Pith downloads the model into `%LOCALAPPDATA%\Pith\data\models`, then shows a tray icon.

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

- Enabled, fix typos, and how often suggestions appear (Fewer, Normal, More)
- Suggestion length (Short, Normal, Long)
- The model (Small, Standard or Large, with the recommended one marked). Choosing one downloads it if needed and switches without a restart; downloaded models are kept
- Ghost text opacity
- Your stats: suggestions seen and accepted today and in total, and the time they saved you
- The apps with their own setting: where Pith is turned off, and the code editors where you turned it on
- Start with Windows (off by default)
- Open data folder, Delete model and data

Settings also warns when something else suggests in the same fields: Windows text suggestions for hardware keyboards (with a button to turn them off), PhraseExpress or Lightkey.

When an app reports no caret, or a caret Pith cannot trust, the suggestion shows in a small pill just below the field, or inside its bottom edge when there is no room. The pill keeps its own background and contrast, so the opacity setting does not apply to it.

The pill is also used when the only text after the caret on the line is closing punctuation, such as `)`, `]`, `"`, `.`, `?`, `」` or `）。` (closing brackets and quotes, and terminal punctuation in any script, by Unicode property). Pith suggests the words that belong before it and shows them in a pill that starts at the caret, just below its line, so the suggestion never sits on top of the punctuation. In a field with several lines the pill covers the start of the line under the caret while it is shown. The pill spans from the caret to the right edge of the field (or of the screen when the app does not report a field) and wraps up to four lines. When the suggestion does not fit, Pith cuts it to the longest run of whole words that does and still fits the rules above, and accepting inserts only those; when not even one word fits on the first row, nothing is shown, the tray says "Skipped: no room for the suggestion", and Pith tries once more when the window or the caret moves and again whenever you edit the text. Resizing a field without moving the caret does not count as a move unless the pill is anchored to the field itself, so Pith usually waits until you type. This applies to every pill and to the ghost text, so what you accept is always what you saw. A suggestion that repeats the punctuation after the caret is dropped (`could you send it` before `?`, not `could you send it?`), and so is one that ends in a comma, semicolon or colon (or the equivalent in another script), ends a sentence in front of punctuation that already ends one (`soon.` before `).`), closes a bracket it did not open or runs over more than one sentence (an abbreviation such as `Dr. Smith` counts as a sentence break, so suggestions containing one are dropped). Accepting inserts at the caret and leaves the punctuation where it is. Any other text after the caret on the line (a word, an opening bracket, a symbol) still skips the suggestion.

Pith pauses and frees the model while Windows battery saver is on.

### Typo fixes

When you type a space after a word the Windows spell checker does not know, Pith shows the correction in a pill under the caret instead of a completion, for example `teh → the`, with the fix in stronger text. Tab (or Shift+Tab or Ctrl+Right, or any key you set in `accept_keys` or `accept_word_keys`) replaces the word and keeps the space and the caret where they were. Esc dismisses it, and so does typing on without taking it: either way that word is not offered again until you restart Pith. The completion for that spot waits while the check runs (the first check of a session can take a moment while Windows loads its dictionaries; Pith waits at most one second) and while the fix is shown, then carries on once you accept, dismiss or type. In manual trigger mode no fix is offered unprompted.

- It works in every language Windows has a spell checker for. Pith asks for the language of your current keyboard layout (`en-US`, `de-DE`, `hi-IN`) and falls back to the language without its region. With no checker for that language, nothing is offered. The language is the layout's, not the text's: words of another language typed on the wrong layout are checked against the layout's dictionary, and the one-edit rule is the only guard against a wrong fix. Words you added to the Windows dictionary count as correct, and your Windows autocorrect list is honoured.
- Only the word just before the caret and the space you typed are checked, and only when the checker finds exactly that word wrong and the best suggestion is one edit away (a swapped, missing, extra or wrong letter) or comes from your autocorrect list. A fix is always a single word of at most 32 letters, so autocorrect entries that expand into several words are ignored. Anything less certain is left alone.
- It never touches a password field, text after the caret, a word under three letters, a word with digits, an ALL-CAPS word or one with capitals inside it (`NASA`, `iPhone`), a capitalised word in the middle of a sentence (names), or a word that is part of a URL, an email address, a path, a hyphenated word or an identifier. Blocked apps, pauses and the per-app switch in the tray apply here as they do to completions.
- Applying a fix selects the word through UI Automation, checks that the selected text is exactly the word you typed, and pastes the correction over it, so it is one undo step. If the app cannot select the word, or the selection does not read back as that word, nothing is replaced and that word is not offered again until you restart Pith. If focus moves to another field or the field cannot be read before the selection happens, nothing is replaced and the word can be offered again. If the field changes while the selection is being made, nothing is pasted, the word is not declined, and the caret is put back if you have not changed the selection yourself. Until the word is selected (at most three seconds), Pith starts no completion or spell check; the paste itself then lands as a normal edit.
- When you type the space while a completion is already showing and the word matches its start, Pith keeps typing through that completion and does not check the word.
- The Fix typos toggle in Settings, or `typo_fix = false`, turns it off.

### Nearby text

Pith adds a few lines from around the box you type in to the start of the prompt, so a reply knows what it replies to: the box's own label, the subject and recipient fields above it, the headings and captions near it and the last message of the conversation above it. Lines are the app's own words in any language, with no labels added by Pith.

- Windows walks the UI Automation tree upwards from the field (at most 6 levels, 12 siblings each, 60 ms in total) and Linux walks the AT-SPI tree the same way. The result is cached per field and window title, so typing costs nothing extra, and a new walk happens when the field or the title changes or when the box is cleared after a send.
- Password boxes, hidden and off-screen items and lines that are mostly symbols or numbers are skipped, and so is anything the title or your own last 200 characters already say.
- The lines go before the text you typed, so the model's cache is reused while you type. They are capped at `nearby_tokens` (300) and by what is left of `context_tokens` after the text you typed. The first suggestion in a field takes a little longer because the lines are read by the model once.
- With `remote_url` set nothing from around the box is sent, only the text you typed as before. `nearby_context = false` turns the whole feature off.

## Verify your download

Every release carries `pith.exe`, a portable zip and `SHA256SUMS`. GitHub Actions also signs a build provenance attestation for `pith.exe` and for the zip (not for `SHA256SUMS`). It says which commit and which workflow of this repository built the file.

Check where the file came from (needs the [GitHub CLI](https://cli.github.com)):

```
gh attestation verify pith.exe --repo kunalgoesbyken/pith --signer-workflow kunalgoesbyken/pith/.github/workflows/release.yml
```

Use the same command with the zip's file name to check the zip.

Check the file was not damaged or swapped in transit, with tools Windows already has:

```
certutil -hashfile pith.exe SHA256
certutil -hashfile pith-0.1.0-x64-portable.zip SHA256
```

Each hash must equal that file's own line in `SHA256SUMS` from the same release (use your version in the zip's name). The checksum alone proves less than the attestation: `SHA256SUMS` sits on the same release page, so whoever could replace the exe could replace the list too.

**About SmartScreen and Smart App Control.** `pith.exe` is not code-signed. Signing certificates cost money every year, and Pith is a free MIT project. SmartScreen judges an unsigned program by how many people have already run that exact file, so a new release shows "Windows protected your PC", and it can show again on the next release because each build is a new file. The warning means "not known yet", not "known bad". After the attestation or the hash checks out, choose More info, then Run anyway. Extracting the zip with Windows Explorer copies the download's mark of the web onto `pith.exe`, so the same prompt appears for the extracted file.

Windows 11 with Smart App Control turned on is stricter: it blocks unsigned programs and offers no Run anyway. Then your choices are to build Pith from source (see Build and test), or to turn Smart App Control off, which Windows only lets you undo by resetting the PC. We have not verified how Smart App Control treats a binary you built yourself.

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
| `typo_fix` | true | offer the correction of a misspelled word you just finished, using the Windows spell checker |
| `blocked_apps` | [] | extra process names to ignore. Terminals, password managers and sign-in screens are always ignored, and so are code editors unless listed in `allowed_apps` |
| `allowed_apps` | [] | code editors to turn Pith on in: `Code`, `Cursor`, `devenv`, `idea64`, `pycharm64`, `rider64`, `zed`. Any other name, including a terminal or password manager, is ignored. Pith completes prose, not code: it skips only lines that are mostly symbols or numbers, so expect suggestions in comments and some in code |
| `nearby_context` | true | read the label, subject and last message around the box to help the suggestion; never password boxes, never sent to a remote server |
| `nearby_tokens` | 300 | most tokens of that text added to the prompt, at most half of `context_tokens` |
| `typing_wpm` | 40 | your typing speed, used for the time-saved estimate (1 to 500) |
| `remote_url` | none | use an OpenAI-compatible `/v1/completions` server instead of the local model; `https`, or `http` to this computer |
| `remote_model` | none | the `model` name sent to that server |

## How it works on Windows

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
- Learns your phrasing from text you accept, kept on this PC
- Context from the rest of the window, such as the email you are replying to
- Suggestions before words in the middle of a line, which needs a model that reads what follows the caret
- Snippets and per-site rules
- Several candidates to choose from
- Code signing, winget and automatic update notices
- macOS (Accessibility API) and Linux on Wayland (Linux on X11 is experimental)

## Linux (X11, experimental)

Run `pith --allow-accessibility` in an X11 session. It refuses to start in a Wayland session. Desktop accessibility must be on; the flag switches it on for the session and `SIGINT` or `SIGTERM` (Ctrl+C, a kill, a logout) restores it. A process that is killed hard leaves it on.

- **Reads text** through AT-SPI from the focused field only. The accessibility bus delivers focus, caret and selection events from every app, but they carry object references and no text; Pith never asks any other field for its text.
- **Captures the accept keys and Esc** with an X key grab only while a suggestion is shown, plus the force-suggestion chord while a field Pith may read is focused. A key it does not act on is handed back to the app unchanged; if Pith does not answer within 150 ms the key goes to the app.
- **Draws** the suggestion in an override-redirect window that never takes the focus or the mouse. With a compositing manager it is inline at the caret, in the field's font and colour at reduced opacity. Without one it is a pill below the caret.
- **Inserts** through the accessibility interface. It falls back to the clipboard only when the field has no editable text interface or refuses the insert. Once a field has accepted an insert Pith never pastes, even if the text cannot be confirmed; it re-reads the field once and takes whatever is there. Firefox, Thunderbird, LibreWolf and Tor Browser accept the insert call but ignore it, so Pith pastes in them from the start. Before either path Pith checks that the focused field and app are still the ones the suggestion was made for.
- **Config and data** live in `~/.config/pith/config.toml` and `~/.local/share/pith/` (the model and `pith.log`), or under `$XDG_CONFIG_HOME` and `$XDG_DATA_HOME`. `blocked_apps` and `allowed_apps` use the lower-case program name, for example `keepassxc`.
- **Build** with `cargo build --release -p pith`. CI runs the end-to-end suite on Xvfb with GTK 3 and Firefox.

Which app is behind a window comes from the process that owns it. Programs run by an interpreter or a sandbox are named as follows, and Pith reads nothing from a window it cannot name:

- Python, Perl, Ruby, Node, Mono, .NET, shell scripts: the script or assembly name, after skipping the interpreter's own options and their values. An option Pith does not know, `-c`, `-m`, `-e`, and a generic entry name (`main`, `app`, `index`, `__main__`, `run`, `start`, `server`, `cli`) give no name, so nothing is read.
- Java: only `-jar` gives a name; a main class does not, so nothing is read.
- Flatpak and Snap: the full application id (`FLATPAK_ID`, or `snap.<name>` for `SNAP_NAME`), matched through a table of known ids such as `org.keepassxc.KeePassXC` to `keepassxc`. An id that is not in the table is off by default. Pith writes it to `pith.log` once; put that exact id in `allowed_apps` to turn it on. Blocked apps and password fields never have their window title read either.
- Apps seen only through the sandbox bridge (`bwrap`, `flatpak-spawn`, `xdg-dbus-proxy`) have no name, so nothing is read.
- A shared Electron binary opening an `.asar` has no name, so nothing is read. An Electron app that ships its own executable is named after it.

Known gaps:

- No tray, settings window, autostart or packaging yet.
- GTK 4 does not report the caret position, so there is no placement in GTK 4 fields.
- The inline ghost wraps against the monitor, not the field.

## License

MIT
