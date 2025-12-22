# flview

A small terminal (TUI) + web file browser.

## Run (TUI)

```bash
cargo run -- tui
```

### Keys

- `Up`/`Down`: move selection
- `Enter`: open directory / open file preview
- `Backspace`: go up one directory
- `t`: set preview mode to text
- `h`: set preview mode to hex
- `PageUp`/`PageDown`: scroll preview
- `q`: quit

## Run (Web)

```bash
cargo run -- web --addr 127.0.0.1:8080
```

Then open `http://127.0.0.1:8080/`.

### Notes

- By default, the root is the current working directory. Use `--root` to choose another root.
- Web mode blocks path traversal; only paths under the root are accessible.

## Run (Site)

Fetch a URL and print TLS negotiation details (for HTTPS) plus response headers.

```bash
cargo run -- site https://example.com/
```

### Options

- `-k`: ignore server certificate validation (HTTPS only)
- `-H 'Name: Value'`: add request header (repeatable)
- `-d 'payload'`: send request body (implies POST)
- `--resolveto <IPV4>`: connect to this IPv4 address instead of resolving the URL hostname via DNS (HTTPS still uses the URL hostname for SNI/cert validation)
