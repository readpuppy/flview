# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`flview` is a dual-mode file browser with three main interfaces:
1. **TUI mode**: Terminal-based file browser using `ratatui` and `crossterm`
2. **Web mode**: HTTP server file browser using `axum` and `tokio`
3. **Site mode**: HTTP/HTTPS client that fetches URLs and displays TLS negotiation details and response headers

## Build and Run Commands

```bash
# Build the project
cargo build

# Run TUI mode (terminal file browser)
cargo run -- tui
cargo run -- tui --root /path/to/dir

# Run web server mode
cargo run -- web --addr 127.0.0.1:8080
cargo run -- web --addr 127.0.0.1:8080 --root /path/to/dir

# Run site mode (HTTP/HTTPS client)
cargo run -- site https://example.com/
cargo run -- site https://example.com/ -k                      # skip cert validation
cargo run -- site https://example.com/ -H 'X-Test: value'     # add header
cargo run -- site https://example.com/ -d 'payload'           # POST with body
cargo run -- site https://example.com/ --resolveto 1.2.3.4    # override DNS

# Format code (run before commits)
cargo fmt

# Lint with clippy (warnings as errors)
cargo clippy -- -D warnings

# Run tests
cargo test

# Run specific test
cargo test test_name
```

## Architecture

The codebase follows a clean separation between UI frontends and shared utilities:

### Module Structure

- **`src/main.rs`**: CLI entry point using `clap`. Parses arguments and dispatches to one of three subcommands (tui/web/site). Handles `--root` flag that sets the browsing root directory.

- **`src/tui.rs`**: Terminal UI implementation using `ratatui` (rendering) and `crossterm` (events). Maintains state for current directory, selected item, and file preview (text/hex modes). Event loop handles navigation keys and preview scrolling.

- **`src/web.rs`**: Axum web server with routes for directory listing (`/`), file viewing (`/view`), file download (`/download`), and file upload (`/upload`). All routes enforce path traversal protection via `fsutil::resolve_under_root`.

- **`src/site.rs`**: HTTP/HTTPS client using `hyper`, `rustls`, and `tokio`. Supports custom headers, POST data, and DNS override (`--resolveto`). The `-k` flag enables insecure mode (bypasses TLS certificate validation).

- **`src/fsutil.rs`**: Shared filesystem utilities used by both TUI and web modes:
  - `list_dir`: Returns sorted directory entries (directories first, case-insensitive names)
  - `read_file_limited`: Reads up to N bytes from a file
  - `canonicalize_root`: Validates and canonicalizes the root path
  - `resolve_under_root`: Path traversal protection - ensures requested paths stay within root

- **`src/hex.rs`**: Hex dump formatting shared by TUI and web modes. Produces classic hex viewer output with offset, hex bytes, and ASCII representation.

### Key Design Patterns

1. **Path Traversal Protection**: All web routes and file operations go through `fsutil::resolve_under_root` which canonicalizes paths and ensures they don't escape the root directory. This is critical for web mode security.

2. **Shared Utilities**: Both TUI and web modes depend on `fsutil` and `hex` modules. When modifying these modules, consider impact on both frontends.

3. **Async/Sync Split**:
   - Web and site modes are fully async (`tokio`)
   - TUI mode is primarily synchronous (blocking I/O is fine for interactive terminal use)
   - `main.rs` uses `#[tokio::main]` to support all three modes

4. **Preview Modes**: Both TUI and web support text and hex preview modes for files. The mode selection affects how file contents are rendered but not how they're read.

5. **Site Mode DNS Override**: The `--resolveto` flag changes the TCP connect address but preserves the original hostname for HTTP `Host` header and HTTPS SNI/certificate validation.

## Development Notes

- Tests are inline using `#[cfg(test)]` modules within each source file
- The project uses standard Rust 2021 edition conventions
- Release profile is optimized for size (LTO, single codegen unit, stripped)
- When adding new filesystem operations, ensure they use `fsutil::resolve_under_root` if user-provided paths are involved
- Web mode blocks absolute paths and path traversal attempts; TUI mode has no such restrictions since it's a local tool
