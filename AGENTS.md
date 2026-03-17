# Repository Guidelines

## Project Structure & Module Organization
`src/` contains the Rust application. `main.rs` handles CLI entry points, `config.rs` loads environment settings, `state.rs` owns shared app state, `search.rs` and `download.rs` wrap `yt-dlp`, `tui/` contains the ratatui interface, and `web/` contains the axum server and API routes. `assets/index.html` is the single-page web client. Runtime output goes to `downloads/` and `logs/`; do not treat those directories as source.

## Build, Test, and Development Commands
Use Cargo for all local workflows:

```bash
cargo build --release   # production build
cargo run               # start the default TUI mode
cargo run -- web        # start the web server on port 3000
cargo run -- web --port 8080
cargo test              # run Rust tests
cargo fmt               # format the codebase
cargo clippy --all-targets --all-features -D warnings
```

The app also depends on external tools: `yt-dlp` and `ffmpeg` must be installed and available on `PATH`.

## Coding Style & Naming Conventions
Follow standard Rust style: 4-space indentation, `snake_case` for functions/modules, `PascalCase` for structs/enums, and small focused modules. Prefer `anyhow::Result` for fallible top-level flows and keep async boundaries explicit. Run `cargo fmt` before submitting changes; use Clippy warnings as the baseline lint standard.

## Testing Guidelines
There is no dedicated `tests/` directory yet, and the current source tree does not include inline tests. Add unit tests next to the module they cover for parsing, configuration, and state transitions, and add integration tests under `tests/` when behavior crosses TUI/Web boundaries. Name tests by behavior, for example `parse_progress_handles_percentages`. Run `cargo test` before opening a PR.

## Commit & Pull Request Guidelines
Recent history mixes Conventional Commit prefixes (`feat:`) with short imperative Chinese summaries. Keep commit subjects short, imperative, and scoped to one change; `feat:`, `fix:`, and `refactor:` are preferred for consistency. PRs should describe user-visible behavior, list required setup changes such as new env vars, and include screenshots or terminal captures when modifying `assets/index.html`, API responses, or the TUI.

## Security & Configuration Tips
Configuration is environment-driven. Verify `DOWNLOAD_DIR`, `PORT`, `YTDLP_PATH`, and `LOG_LEVEL` before testing, and avoid committing machine-specific paths, downloaded media, or log files.
