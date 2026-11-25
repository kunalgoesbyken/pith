# Numbers

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

## Per model

| `model` | File | Download | RAM loaded | Median / p90 per keystroke | Keystrokes saved | Exact next word | Notes |
|---|---|---|---|---|---|---|---|
| `small` | SmolLM2-360M Q4_K_M | 271 MB | ~370 MB | 148 / 306 ms | 26% | 11% | English-focused, loads in 0.65 s |
| `standard` | Qwen3-0.6B-Base Q4_K_M | 397 MB | ~590 MB | 206 / 420 ms | 29% | 12% | multilingual, the common choice |
| `large` | Qwen3-1.7B-Base Q4_K_M | 1107 MB | ~1270 MB | 520 / 1316 ms | 37% | 14% | multilingual, best suggestions, about 2.5 times slower than standard |

Measured with `typing_bench` on one PC (4 threads, 8 logical cores, 16 GB; 1,200 to 1,400 keystrokes over 12 samples). RAM is the peak private memory of the process while loaded. Latency depends on your CPU, so read the milliseconds as relative. The figures above are from a slower laptop.

Pith recommends a model from the memory and logical cores it finds: `large` with at least 13.2 GB of RAM and 12 cores, `standard` with at least 5.6 GB and 4 cores, `small` otherwise. Until you choose one in Settings, Pith uses the recommendation.
