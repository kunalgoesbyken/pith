# Config

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
| `style_memory` | false | learn the finished sentences in which you accept a suggestion and use the latest three from the same app as examples; stored encrypted on Windows, never sent |
| `typing_wpm` | 40 | your typing speed, used for the time-saved estimate (1 to 500) |
| `remote_url` | none | use an OpenAI-compatible `/v1/completions` server instead of the local model; `https`, or `http` to this computer |
| `remote_model` | none | the `model` name sent to that server |
