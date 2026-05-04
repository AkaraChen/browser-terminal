# Browser Terminal

A small local terminal server that lets the browser act as a terminal window manager.

The Rust backend serves a single HTML page and forwards WebSocket traffic to an
independent PTY session. The frontend uses Xterm.js, reads a `channel` query
parameter, and opens each new browser tab as a separate terminal session.

## Features

- Browser terminal UI powered by Xterm.js
- Rust WebSocket backend using Axum
- One PTY session per WebSocket connection
- `?channel=<id>` URL-based session identity
- New-tab button for opening a fresh terminal session
- Settings dialog for terminal font and font size
- Settings persisted in `localStorage`
- Live settings sync across open tabs via the browser `storage` event
- Window title sync from Xterm title updates
- Dynamic document icon for Claude, Codex, OpenCode, Qoder, Amp, Cline,
  Copilot, Cursor, Kilo Code, and Kimi terminal titles
- Tailwind CDN for compact page styling

## Run

```sh
cargo run
```

Then open:

```text
http://127.0.0.1:3000/?channel=main
```

Use a different port with:

```sh
PORT=3100 cargo run
```

## Notes

The server binds to `127.0.0.1` and is intended for local use. It does not
include authentication, so do not expose it directly to the public internet.

Shells start in the user's home directory. The PTY implementation uses
`portable-pty`, and the home directory is resolved with the cross-platform
`dirs` crate.

## Tech Stack

- Rust
- Axum
- Tokio
- portable-pty
- Xterm.js
- Tailwind CSS CDN
- LobeHub Icons static SVG CDN
