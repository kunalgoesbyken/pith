# Pith

Local autocomplete for every text box. Pause while you type and Pith shows the rest of your sentence in grey at the caret.

- Tab keeps all of it, Shift+Tab or Ctrl+Right keeps one word, Esc hides it.
- Ctrl+Shift+Space asks for a suggestion right away.
- After a misspelled word it offers the fix instead (`teh → the`), and Tab applies it.

A small model runs on your CPU. No account, no telemetry, no GPU; 8 GB of RAM is enough. Windows today, Linux with X11 is experimental, macOS is planned.

Website: [pith-smoky.vercel.app](https://pith-smoky.vercel.app)

## Install

Download `pith.exe` from [Releases](https://github.com/kunalgoesbyken/pith/releases) and run it. On first start it downloads a model and shows a tray icon. The exe is not code-signed yet, so Windows may ask you to confirm; [verify the download](docs/verify.md) first if you like.

Pith needs a CPU with AVX2 (most PCs since 2015).

## Privacy

- What you type is read from the focused field only, kept in memory, never saved or sent.
- The network is used once for the model download, and for update checks you start yourself.
- Password fields, password managers and sign-in screens are never read.
- On Windows there is no keyboard hook, so Pith never sees your keystrokes.
- No telemetry.

The full table is in [docs/privacy.md](docs/privacy.md).

## Models

| Model | Download | Memory | Per keystroke | Keystrokes saved |
|---|---|---|---|---|
| Small, English first | 271 MB | ~370 MB | 148 ms | 26% |
| Standard, many languages | 397 MB | ~590 MB | 206 ms | 29% |
| Large, best suggestions | 1107 MB | ~1270 MB | 520 ms | 37% |

Pith picks one from your RAM and cores, and you can switch in Settings. Median on one 8-core PC; more in [docs/benchmarks.md](docs/benchmarks.md).

## Linux

Experimental, X11 only: run `pith --allow-accessibility`. See [docs/linux.md](docs/linux.md).

## Docs

- [Using Pith](docs/using-pith.md): the tray, Settings, the pill, typo fixes, nearby text and your style
- [Config](docs/config.md): every key in `config.toml`
- [How it works](docs/architecture.md): internals, building, tests and the roadmap

## Build

```
cargo build --release
cargo test --workspace
```

Needs Rust (MSVC toolchain), CMake and libclang. Details in [docs/architecture.md](docs/architecture.md#build-and-test).

## License

[GPL-3.0](LICENSE)
