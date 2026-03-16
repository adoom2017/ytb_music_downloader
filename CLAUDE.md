# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A YouTube music batch downloader written in Rust, supporting two modes:
- **TUI mode**: Terminal UI using ratatui for local interactive use
- **Web mode**: Browser-based UI via axum web server for remote access

Uses [yt-dlp](https://github.com/yt-dlp/yt-dlp) as the core download engine.

## Common Commands

```bash
# Build release binary
cargo build --release

# Run in TUI mode (default)
cargo run

# Run in Web mode
cargo run -- web
cargo run -- web --port 8080  # custom port
```

## Architecture

The application is structured around a shared `AppState` that both TUI and Web modes use:

- **`src/main.rs`**: CLI entry point with `tui`/`web` subcommands
- **`src/config.rs`**: Configuration management via environment variables
- **`src/state.rs`**: Shared application state (`SharedState = Arc<Mutex<AppState>>`) containing search results, download tasks, and log handle
- **`src/search.rs`**: yt-dlp integration for YouTube search
- **`src/download.rs`**: yt-dlp download execution with progress parsing
- **`src/tui/`**: Terminal UI using ratatui (event loop + rendering)
- **`src/web/`**: REST API server using axum + frontend in `assets/index.html`
- **`src/logger.rs`**: Tracing-based logging with file rotation

The TUI and Web modes share the same state, meaning downloads initiated from one interface appear in the other.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DOWNLOAD_DIR` | `./downloads` | Download save directory |
| `PORT` | `3000` | Web server port |
| `YTDLP_PATH` | `yt-dlp` | yt-dlp executable path |
| `LOG_LEVEL` | `info` | Log level (trace/debug/info/warn/error) |

## External Dependencies

- **yt-dlp**: Must be installed separately (`pip install yt-dlp`)
- **ffmpeg**: Required for audio conversion (`brew install ffmpeg` on macOS)
